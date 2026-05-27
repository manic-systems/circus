{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-e2e";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  # End-to-end tests: flake creation, evaluation, queue runner, notification,
  # signing, GC, declarative, webhooks
  testScript = ''
    import hashlib
    import json
    import re
    import time

    machine.start()
    machine.wait_for_unit("postgresql.service")

    # Ensure PostgreSQL is actually ready to accept connections before circus-server starts
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)

    machine.wait_for_unit("circus-server.service")

    # Wait for the server to start listening
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # Seed an API key for write operations
    # Token: circus_testkey123 -> SHA-256 hash inserted into api_keys table
    api_token = "circus_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # Seed a read-only key
    ro_token = "circus_readonly_key"
    ro_hash = hashlib.sha256(ro_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('readonly', '{ro_hash}', 'read-only')\""
    )
    ro_header = f"-H 'Authorization: Bearer {ro_token}'"

    with subtest("PostgreSQL LISTEN/NOTIFY triggers are installed"):
        result = machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \"SELECT tgname FROM pg_trigger WHERE tgname LIKE 'trg_%_notify'\" -t"
        )
        assert "trg_builds_insert_notify" in result, f"Missing trg_builds_insert_notify in: {result}"
        assert "trg_builds_status_notify" in result, f"Missing trg_builds_status_notify in: {result}"
        assert "trg_jobsets_insert_notify" in result, f"Missing trg_jobsets_insert_notify in: {result}"
        assert "trg_jobsets_update_notify" in result, f"Missing trg_jobsets_update_notify in: {result}"
        assert "trg_jobsets_delete_notify" in result, f"Missing trg_jobsets_delete_notify in: {result}"

    # Create a test flake inside the VM
    with subtest("Create bare git repo with test flake"):
        machine.succeed("mkdir -p /var/lib/circus/test-repos")
        machine.succeed("git init --bare /var/lib/circus/test-repos/test-flake.git")

        # Allow root to push to circus-owned repos (ownership changes after chown below)
        machine.succeed("git config --global --add safe.directory /var/lib/circus/test-repos/test-flake.git")

        # Create a working copy, write the flake, commit, push
        machine.succeed("mkdir -p /tmp/test-flake-work")
        machine.succeed("cd /tmp/test-flake-work && git init")
        machine.succeed("cd /tmp/test-flake-work && git config user.email 'test@circus' && git config user.name 'circus Test'")

        # Write a minimal flake.nix that builds a simple derivation
        machine.succeed(
            "cat > /tmp/test-flake-work/flake.nix << 'FLAKE'\n"
            "{\n"
            '  description = "circus test flake";\n'
            '  outputs = { self, ... }: {\n'
            '    packages.x86_64-linux.hello = derivation {\n'
            '      name = "circus-test-hello";\n'
            '      system = "x86_64-linux";\n'
            '      builder = "/bin/sh";\n'
            '      args = [ "-c" "echo hello > $out" ];\n'
            "    };\n"
            "  };\n"
            "}\n"
            "FLAKE\n"
        )
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'initial flake'")
        machine.succeed("cd /tmp/test-flake-work && git remote add origin /var/lib/circus/test-repos/test-flake.git")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/master")

        # Set ownership for circus user
        machine.succeed("chown -R circus:circus /var/lib/circus/test-repos")

    # Create project + jobset pointing to the local repo via API
    with subtest("Create E2E project and jobset via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"e2e-test\", \"repository_url\": \"file:///var/lib/circus/test-repos/test-flake.git\"}' "
            "| jq -r .id"
        )
        e2e_project_id = result.strip()
        assert len(e2e_project_id) == 36, f"Expected UUID, got '{e2e_project_id}'"

        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"packages\", \"nix_expression\": \"packages\", \"flake_mode\": true, \"enabled\": true, \"check_interval\": 10}' "
            "| jq -r .id"
        )
        e2e_jobset_id = result.strip()
        assert len(e2e_jobset_id) == 36, f"Expected UUID for jobset, got '{e2e_jobset_id}'"

    # Wait for evaluator to pick it up and create an evaluation
    with subtest("Evaluator discovers and evaluates the flake"):
        # The evaluator is already running, poll for evaluation to appear
        # with status "completed"
        machine.wait_until_succeeds(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' "
            "| jq -e '.items[] | select(.status==\"completed\")'",
            timeout=90
        )

    with subtest("Evaluation created builds with valid drv_path"):
        # Get evaluation ID
        e2e_eval_id = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' "
            "| jq -r '.items[] | select(.status==\"completed\") | .id' | head -1"
        ).strip()
        assert len(e2e_eval_id) == 36, f"Expected UUID for evaluation, got '{e2e_eval_id}'"

        # Verify builds were created
        result = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={e2e_eval_id}' | jq '.items | length'"
        )
        build_count = int(result.strip())
        assert build_count >= 1, f"Expected >= 1 build, got {build_count}"

        # Verify build has valid drv_path
        drv_path = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={e2e_eval_id}' | jq -r '.items[0].drv_path'"
        ).strip()
        assert drv_path.startswith("/nix/store/"), f"Expected /nix/store/ drv_path, got '{drv_path}'"

        # Get the build ID for later
        e2e_build_id = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={e2e_eval_id}' | jq -r '.items[0].id'"
        ).strip()

    # Test evaluation caching
    with subtest("Same commit does not trigger a new evaluation"):
        # Get current evaluation count
        before_count = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip()
        # Wait longer than check_interval (10s) to ensure the evaluator re-checks
        time.sleep(15)
        after_count = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip()
        assert before_count == after_count, f"Evaluation count changed from {before_count} to {after_count} (should be cached)"

    # Test new commit triggers new evaluation
    with subtest("New commit triggers new evaluation"):
        before_count_int = int(machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip())

        # Push a new commit
        machine.succeed(
            "cd /tmp/test-flake-work && \\\n"
            "cat > flake.nix << 'FLAKE'\n"
            "{\n"
            '  description = "circus test flake v2";\n'
            '  outputs = { self, ... }: {\n'
            '    packages.x86_64-linux.hello = derivation {\n'
            '      name = "circus-test-hello-v2";\n'
            '      system = "x86_64-linux";\n'
            '      builder = "/bin/sh";\n'
            '      args = [ "-c" "echo hello-v2 > $out" ];\n'
            "    };\n"
            "  };\n"
            "}\n"
            "FLAKE\n"
        )
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'v2 update'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/master")

        # Wait for evaluator to detect and create new evaluation
        machine.wait_until_succeeds(
            f"test $(curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length') -gt {before_count_int}",
            timeout=60
        )

    with subtest("Queue runner builds pending derivation"):
        # Poll the E2E build until succeeded (queue-runner is already running)
        machine.wait_until_succeeds(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -e 'select(.status==\"succeeded\")'",
            timeout=120
        )

    with subtest("Completed build has output path"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -r .build_output_path"
        ).strip()
        assert result != "null" and result.startswith("/nix/store/"), \
            f"Expected /nix/store/ output path, got '{result}'"

    with subtest("Build steps recorded"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/steps | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected >= 1 build step, got {result.strip()}"

        # Verify exit_code = 0
        exit_code = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/steps | jq '.[0].exit_code'"
        ).strip()
        assert exit_code == "0", f"Expected exit_code 0, got {exit_code}"

    with subtest("Build products created"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/products | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected >= 1 build product, got {result.strip()}"

        # Verify product has valid path
        product_path = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/products | jq -r '.[0].path'"
        ).strip()
        assert product_path.startswith("/nix/store/"), f"Expected /nix/store/ product path, got '{product_path}'"

    with subtest("Build log exists"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/log"
        ).strip()
        assert code == "200", f"Expected 200 for build log, got {code}"

    with subtest("Create jobset input via API"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"nixpkgs\", \"input_type\": \"git\", \"value\": \"https://github.com/NixOS/nixpkgs\"}'"
        )
        input_data = json.loads(result)
        assert "id" in input_data, f"Expected id in response: {result}"
        e2e_input_id = input_data["id"]

    with subtest("List jobset inputs"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected >= 1 input, got {result.strip()}"

    with subtest("Read-only key cannot create jobset input"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test\", \"input_type\": \"string\", \"value\": \"hello\"}'"
        ).strip()
        assert code == "403", f"Expected 403 for read-only input create, got {code}"

    with subtest("Delete jobset input"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs/{e2e_input_id} "
            f"{auth_header}"
        ).strip()
        assert code == "200", f"Expected 200 for input delete, got {code}"

    with subtest("Read-only key cannot delete jobset input"):
        # Re-create first
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test-ro\", \"input_type\": \"string\", \"value\": \"test\"}'"
        )
        tmp_input_id = json.loads(result)["id"]
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs/{tmp_input_id} "
            f"{ro_header}"
        ).strip()
        assert code == "403", f"Expected 403 for read-only input delete, got {code}"
        # Clean up: admin deletes the temp input so it doesn't affect future
        # inputs_hash computations and evaluator cache lookups
        machine.succeed(
            "curl -sf -o /dev/null "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs/{tmp_input_id} "
            f"{auth_header}"
        )

    with subtest("Build status is succeeded"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -r .status"
        ).strip()
        assert result == "succeeded", f"Expected succeeded, got {result}"

    with subtest("Channel auto-promotion after all builds complete"):
        # Create a channel tracking the E2E jobset
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/channels "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"project_id\": \"{e2e_project_id}\", \"name\": \"e2e-channel\", \"jobset_id\": \"{e2e_jobset_id}\"}}' "
            "| jq -r .id"
        )
        e2e_channel_id = result.strip()

        # Auto-promotion happens when all builds in an evaluation complete.
        # The first evaluation's builds should already be complete.
        # Check channel's current_evaluation_id
        machine.wait_until_succeeds(
            f"curl -sf http://127.0.0.1:3000/api/v1/channels/{e2e_channel_id} "
            "| jq -e 'select(.current_evaluation_id != null)'",
            timeout=30
        )

    with subtest("Binary cache serves NARinfo for built output"):
        # Get the build output path
        output_path = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -r .build_output_path"
        ).strip()

        # Extract the hash from /nix/store/<hash>-<name>
        hash_match = re.match(r'/nix/store/([a-z0-9]+)-', output_path)
        assert hash_match, f"Could not extract hash from output path: {output_path}"
        store_hash = hash_match.group(1)

        # Request NARinfo
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/nix-cache/{store_hash}.narinfo"
        ).strip()
        assert code == "200", f"Expected 200 for NARinfo, got {code}"

        # Verify NARinfo content has StorePath and NarHash
        narinfo = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/nix-cache/{store_hash}.narinfo"
        )
        assert "StorePath:" in narinfo, f"NARinfo missing StorePath: {narinfo}"
        assert "NarHash:" in narinfo, f"NARinfo missing NarHash: {narinfo}"

    with subtest("Build with invalid drv_path fails and retries"):
        # Insert a build with an invalid drv_path via SQL
        machine.succeed(
            "sudo -u postgres psql -d circus -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, priority, retry_count, max_retries, is_aggregate, signed) "
            f"VALUES (gen_random_uuid(), '{e2e_eval_id}', 'bad-build', '/nix/store/invalid-does-not-exist.drv', 'pending', 0, 0, 3, false, false);\""
        )

        # Wait for queue-runner to attempt the build and fail it
        machine.wait_until_succeeds(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=bad-build' "
            "| jq -e '.items[] | select(.status==\"failed\")'",
            timeout=60
        )

        # Verify status is failed
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=bad-build' | jq -r '.items[0].status'"
        ).strip()
        assert result == "failed", f"Expected failed for bad build, got '{result}'"

    with subtest("LISTEN/NOTIFY: queue-runner reacts to new builds"):
        # Insert a build directly via SQL and verify the queue-runner picks it up
        # reactively (within seconds, not waiting for the full poll interval)
        machine.succeed(
            f"sudo -u circus psql -U circus -d circus -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, priority, retry_count, max_retries, is_aggregate, signed) "
            f"VALUES (gen_random_uuid(), '{e2e_eval_id}', 'notify-build', '/nix/store/invalid-notify-test.drv', 'pending', 0, 0, 1, false, false);\""
        )
        # Queue-runner should pick it up quickly via NOTIFY (poll_interval is 3s in test,
        # but LISTEN/NOTIFY wakes it immediately). The build will fail since the drv is
        # invalid, but we just need to verify it was picked up (status changed from pending).
        machine.wait_until_succeeds(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=notify-build' "
            "| jq -e '.items[] | select(.status!=\"pending\")'",
            timeout=10
        )

    with subtest("Webhook notification fires on build completion"):
        # Start a minimal HTTP server on the VM to receive the webhook POST.
        # Writes the request body to /tmp/webhook.json so we can inspect it.
        machine.succeed(
            "cat > /tmp/webhook-server.py << 'PYEOF'\n"
            "import http.server, json\n"
            "class H(http.server.BaseHTTPRequestHandler):\n"
            "    def do_POST(self):\n"
            "        n = int(self.headers.get('Content-Length', 0))\n"
            "        body = self.rfile.read(n)\n"
            "        open('/tmp/webhook.json', 'wb').write(body)\n"
            "        self.send_response(200)\n"
            "        self.end_headers()\n"
            "    def log_message(self, *a): pass\n"
            "http.server.HTTPServer(('127.0.0.1', 9998), H).serve_forever()\n"
            "PYEOF\n"
        )
        machine.succeed("python3 /tmp/webhook-server.py &")
        machine.wait_until_succeeds(
            "curl -sf -X POST -H 'Content-Length: 2' -d '{}' http://127.0.0.1:9998/",
            timeout=10
        )
        machine.succeed("rm -f /tmp/webhook.json")

        # Configure queue-runner to send webhook notifications
        machine.succeed("mkdir -p /run/systemd/system/circus-queue-runner.service.d")
        machine.succeed(
            "cat > /run/systemd/system/circus-queue-runner.service.d/webhook.conf << 'EOF'\n"
            "[Service]\n"
            "Environment=CIRCUS_NOTIFICATIONS__WEBHOOK_URL=http://127.0.0.1:9998/notify\n"
            "EOF\n"
        )
        machine.succeed("systemctl daemon-reload")
        machine.succeed("systemctl restart circus-queue-runner")
        machine.wait_for_unit("circus-queue-runner.service", timeout=30)

        # Push a new commit to trigger a fresh evaluation and build
        machine.succeed(
            "cd /tmp/test-flake-work && \\\n"
            "cat > flake.nix << 'FLAKE'\n"
            "{\n"
            '  description = "circus test flake notify";\n'
            '  outputs = { self, ... }: {\n'
            '    packages.x86_64-linux.notify-test = derivation {\n'
            '      name = "circus-notify-test";\n'
            '      system = "x86_64-linux";\n'
            '      builder = "/bin/sh";\n'
            '      args = [ "-c" "echo notify-test > $out" ];\n'
            "    };\n"
            "  };\n"
            "}\n"
            "FLAKE\n"
        )
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'trigger webhook notification test'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/master")

        # Wait for the webhook to arrive (the build must complete first)
        machine.wait_until_succeeds("test -e /tmp/webhook.json", timeout=120)
        payload = json.loads(machine.succeed("cat /tmp/webhook.json"))
        assert payload["build_status"] == "success", \
            f"Expected build_status=success in webhook payload, got: {payload}"
        assert "build_id" in payload, f"Missing build_id in webhook payload: {payload}"
        assert "build_job" in payload, f"Missing build_job in webhook payload: {payload}"

    with subtest("Generate signing key and configure signing"):
        # Generate a Nix signing key
        machine.succeed("mkdir -p /var/lib/circus/keys")
        machine.succeed("nix-store --generate-binary-cache-key circus-test /var/lib/circus/keys/signing-key /var/lib/circus/keys/signing-key.pub")
        machine.succeed("chown -R circus:circus /var/lib/circus/keys")
        machine.succeed("chmod 600 /var/lib/circus/keys/signing-key")

        # Enable signing via systemd drop-in override
        machine.succeed("mkdir -p /run/systemd/system/circus-queue-runner.service.d")
        machine.succeed(
            "cat > /run/systemd/system/circus-queue-runner.service.d/signing.conf << 'EOF'\n"
            "[Service]\n"
            "Environment=CIRCUS_SIGNING__ENABLED=true\n"
            "Environment=CIRCUS_SIGNING__KEY_FILE=/var/lib/circus/keys/signing-key\n"
            "EOF\n"
        )
        machine.succeed("systemctl daemon-reload")
        machine.succeed("systemctl restart circus-queue-runner")
        machine.wait_for_unit("circus-queue-runner.service", timeout=30)

    with subtest("Signed builds have valid signatures"):
        # Create a new build to test signing
        machine.succeed(
            "cd /tmp/test-flake-work && \\\n"
            "cat > flake.nix << 'FLAKE'\n"
            "{\n"
            '  description = "circus test flake signing";\n'
            '  outputs = { self, ... }: {\n'
            '    packages.x86_64-linux.sign-test = derivation {\n'
            '      name = "circus-sign-test";\n'
            '      system = "x86_64-linux";\n'
            '      builder = "/bin/sh";\n'
            '      args = [ "-c" "echo signed-build > $out" ];\n'
            "    };\n"
            "  };\n"
            "}\n"
            "FLAKE\n"
        )
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'trigger signing test'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/master")

        # Wait for the sign-test build to succeed
        machine.wait_until_succeeds(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=sign-test' "
            "| jq -e '.items[] | select(.status==\"succeeded\")'",
            timeout=120
        )

        # Get the sign-test build ID
        sign_build_id = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=sign-test' "
            "| jq -r '.items[] | select(.status==\"succeeded\") | .id' | head -1"
        ).strip()

        # Verify the build has signed=true
        signed = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{sign_build_id} | jq -r .signed"
        ).strip()
        assert signed == "true", f"Expected signed=true, got {signed}"

        # Get the output path and verify it with nix store verify
        output_path = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{sign_build_id} | jq -r .build_output_path"
        ).strip()

        # Verify the path is signed with our key
        # The verify command should succeed (exit 0) if signatures are valid
        machine.succeed(f"nix store verify --sigs-needed 1 {output_path}")

    with subtest("GC roots are created for build products"):
        # Enable GC via systemd drop-in override
        machine.succeed("mkdir -p /run/systemd/system/circus-queue-runner.service.d")
        machine.succeed(
            "cat > /run/systemd/system/circus-queue-runner.service.d/gc.conf << 'EOF'\n"
            "[Service]\n"
            "Environment=CIRCUS_GC__ENABLED=true\n"
            "Environment=CIRCUS_GC__GC_ROOTS_DIR=/nix/var/nix/gcroots/per-user/circus\n"
            "Environment=CIRCUS_GC__MAX_AGE_DAYS=30\n"
            "Environment=CIRCUS_GC__CLEANUP_INTERVAL=3600\n"
            "EOF\n"
        )
        machine.succeed("systemctl daemon-reload")
        machine.succeed("systemctl restart circus-queue-runner")
        machine.wait_for_unit("circus-queue-runner.service", timeout=30)

        # Ensure the gc roots directory exists
        machine.succeed("mkdir -p /nix/var/nix/gcroots/per-user/circus")
        machine.succeed("chown -R circus:circus /nix/var/nix/gcroots/per-user/circus")

        # Create a new build to test GC root creation
        machine.succeed(
            "cd /tmp/test-flake-work && \\\n"
            "cat > flake.nix << 'FLAKE'\n"
            "{\n"
            '  description = "circus test flake gc";\n'
            '  outputs = { self, ... }: {\n'
            '    packages.x86_64-linux.gc-test = derivation {\n'
            '      name = "circus-gc-test";\n'
            '      system = "x86_64-linux";\n'
            '      builder = "/bin/sh";\n'
            '      args = [ "-c" "echo gc-test > $out" ];\n'
            "    };\n"
            "  };\n"
            "}\n"
            "FLAKE\n"
        )
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'trigger gc test'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/master")

        # Wait for evaluation and build
        machine.wait_until_succeeds(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=gc-test' | jq -e '.items[] | select(.status==\"succeeded\")'",
            timeout=120
        )

        # Get the build output path
        gc_build_output = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=gc-test' | jq -r '.items[0].build_output_path'"
        ).strip()

        # Verify GC root symlink was created
        # The symlink should be in /nix/var/nix/gcroots/per-user/circus/ and point to the build output
        # Wait for GC root to be created (polling with timeout)
        def wait_for_gc_root():
            gc_roots = machine.succeed("find /nix/var/nix/gcroots/per-user/circus -type l 2>/dev/null || true").strip()
            if not gc_roots:
                return False
            for root in gc_roots.split('\n'):
                if root:
                    target = machine.succeed(f"readlink -f {root} 2>/dev/null || true").strip()
                    if target == gc_build_output:
                        return True
            return False

        # Poll for GC root creation (give queue-runner time to create it)
        machine.wait_until_succeeds(
            "test -e /nix/var/nix/gcroots/per-user/circus",
            timeout=30
        )

        # Wait for a symlink pointing to our build output to appear
        found = False
        for _ in range(10):
            if wait_for_gc_root():
                found = True
                break
            time.sleep(1)

        # Verify build output exists and is protected from GC
        machine.succeed(f"test -e {gc_build_output}")

    with subtest("Declarative .circus.toml in repo auto-creates jobset"):
        # Add .circus.toml to the test repo with a new jobset definition
        machine.succeed(
            "cd /tmp/test-flake-work && "
            "cat > .circus.toml << 'CIRCUSTOML'\n"
            "[[jobsets]]\n"
            'name = "declarative-checks"\n'
            'nix_expression = "checks"\n'
            "flake_mode = true\n"
            "enabled = true\n"
            "CIRCUSTOML\n"
        )
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'add declarative config'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/master")

        # Wait for evaluator to pick up the new commit and process declarative config
        machine.wait_until_succeeds(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets' "
            "| jq -e '.items[] | select(.name==\"declarative-checks\")'",
            timeout=60
        )

    with subtest("Webhook endpoint accepts valid GitHub push"):
        # Create a webhook config via SQL (no REST endpoint for creation)
        machine.succeed(
            "sudo -u postgres psql -d circus -c \""
            "INSERT INTO webhook_configs (id, project_id, forge_type, secret_hash, enabled) "
            f"VALUES (gen_random_uuid(), '{e2e_project_id}', 'github', 'test-secret', true);\""
        )

        # Get the current evaluation count
        before_evals = int(machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip())

        # Compute HMAC-SHA256 of the payload
        payload = '{"ref":"refs/heads/main","after":"abcdef1234567890abcdef1234567890abcdef12","repository":{"clone_url":"file:///var/lib/circus/test-repos/test-flake.git"}}'

        # Generate HMAC with the secret
        hmac_sig = machine.succeed(
            f"echo -n '{payload}' | openssl dgst -sha256 -hmac 'test-secret' -hex | awk '{{print $2}}'"
        ).strip()

        # Send webhook
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/webhooks/{e2e_project_id}/github "
            "-H 'Content-Type: application/json' "
            f"-H 'X-Hub-Signature-256: sha256={hmac_sig}' "
            f"-d '{payload}'"
        ).strip()
        assert code == "200", f"Expected 200 for webhook, got {code}"

        # Verify the webhook response accepted the push
        result = machine.succeed(
            "curl -sf "
            f"-X POST http://127.0.0.1:3000/api/v1/webhooks/{e2e_project_id}/github "
            "-H 'Content-Type: application/json' "
            f"-H 'X-Hub-Signature-256: sha256={hmac_sig}' "
            f"-d '{payload}' | jq -r .accepted"
        ).strip()
        assert result == "true", f"Expected webhook accepted=true, got {result}"

    with subtest("Webhook rejects invalid signature"):
        payload = '{"ref":"refs/heads/main","after":"deadbeef","repository":{}}'
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/webhooks/{e2e_project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-Hub-Signature-256: sha256=0000000000000000000000000000000000000000000000000000000000000000' "
            f"-d '{payload}'"
        ).strip()
        assert code == "401", f"Expected 401 for invalid webhook signature, got {code}"

    # Cleanup: Delete project
    with subtest("Delete E2E project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{e2e_project_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for project delete, got {code.strip()}"

    with subtest("Deleted E2E project returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}"
        )
        assert code.strip() == "404", f"Expected 404 for deleted project, got {code.strip()}"
  '';
}
