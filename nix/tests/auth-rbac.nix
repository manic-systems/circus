{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-auth-rbac";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  # Authentication and RBAC tests
  testScript = ''
    import hashlib
    import json

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

    # Authentication tests
    with subtest("Unauthenticated POST returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"unauth-test\", \"repository_url\": \"https://example.com/repo\"}'"
        )
        assert code.strip() == "401", f"Expected 401, got {code.strip()}"

    with subtest("Wrong token POST returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            "-H 'Authorization: Bearer circus_wrong_token_here' "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"bad-auth-test\", \"repository_url\": \"https://example.com/repo\"}'"
        )
        assert code.strip() == "401", f"Expected 401, got {code.strip()}"

    with subtest("Valid token POST returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"auth-test-project\", \"repository_url\": \"https://example.com/auth-repo\"}'"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    with subtest("GET without token returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    ## RBAC tests
    # Seed a read-only key
    ro_token = "circus_readonly_key"
    ro_hash = hashlib.sha256(ro_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('readonly', '{ro_hash}', 'read-only')\""
    )
    ro_header = f"-H 'Authorization: Bearer {ro_token}'"

    with subtest("Read-only key POST project returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"ro-attempt\", \"repository_url\": \"https://example.com/ro\"}'"
        )
        assert code.strip() == "403", f"Expected 403, got {code.strip()}"

    with subtest("Read-only key POST admin/builders returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/admin/builders "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"bad-builder\", \"ssh_uri\": \"ssh://x@y\", \"systems\": [\"x86_64-linux\"]}'"
        )
        assert code.strip() == "403", f"Expected 403, got {code.strip()}"

    with subtest("Admin key POST admin/builders returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/admin/builders "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test-builder\", \"ssh_uri\": \"ssh://nix@builder\", \"systems\": [\"x86_64-linux\"], \"max_jobs\": 2}'"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    with subtest("Admin key create and delete API key"):
        # Create
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/api-keys "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"ephemeral\", \"role\": \"read-only\"}'"
        )
        key_data = json.loads(result)
        assert "id" in key_data, f"Expected id in response: {result}"
        key_id = key_data["id"]
        # Delete
        code = machine.succeed(
            f"curl -s -o /dev/null -w '%{{http_code}}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/api-keys/{key_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    # API key lifecycle test
    with subtest("API key lifecycle: create, use, delete, verify 401"):
        # Create a new key via admin API
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/api-keys "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"lifecycle-test\", \"role\": \"admin\"}'"
        )
        lc_data = json.loads(result)
        lc_key = lc_data["key"]
        lc_id = lc_data["id"]
        lc_header = f"-H 'Authorization: Bearer {lc_key}'"

        # Use the new key to create a project
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{lc_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"lifecycle-project\", \"repository_url\": \"https://example.com/lc\"}'"
        )
        assert code.strip() == "200", f"Expected 200 with new key, got {code.strip()}"

        # Delete the key
        machine.succeed(
            f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/api-keys/{lc_id} "
            f"{auth_header}"
        )

        # Verify deleted key returns 401
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{lc_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"should-fail\", \"repository_url\": \"https://example.com/fail\"}'"
        )
        assert code.strip() == "401", f"Expected 401 after key deletion, got {code.strip()}"

    # CRUD lifecycle test
    with subtest("CRUD lifecycle: project -> jobset -> list -> delete -> 404"):
        # Create project
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"crud-test\", \"repository_url\": \"https://example.com/crud\"}' "
            "| jq -r .id"
        )
        crud_project_id = result.strip()

        # Create jobset
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{crud_project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"main\", \"nix_expression\": \".\"}' "
            "| jq -r .id"
        )
        jobset_id = result.strip()
        assert len(jobset_id) == 36, f"Expected UUID for jobset, got '{jobset_id}'"

        # List jobsets (should have at least 1)
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{crud_project_id}/jobsets | jq '.items | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 jobset, got {result.strip()}"

        # Delete project (cascades)
        machine.succeed(
            f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{crud_project_id} "
            f"{auth_header}"
        )

        # Verify project returns 404
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/api/v1/projects/{crud_project_id}"
        )
        assert code.strip() == "404", f"Expected 404 after deletion, got {code.strip()}"

    # Edge case tests
    with subtest("Duplicate project name returns 409"):
        machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"dup-test\", \"repository_url\": \"https://example.com/dup\"}'"
        )
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"dup-test\", \"repository_url\": \"https://example.com/dup2\"}'"
        )
        assert code.strip() == "409", f"Expected 409 for duplicate, got {code.strip()}"

    with subtest("Invalid UUID path returns 400"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects/not-a-uuid"
        )
        assert code.strip() == "400", f"Expected 400 for invalid UUID, got {code.strip()}"

    with subtest("XSS in project name returns 400"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"<script>alert(1)</script>\", \"repository_url\": \"https://example.com/xss\"}'"
        )
        assert code.strip() == "400", f"Expected 400 for XSS name, got {code.strip()}"

    # Security fuzzing
    with subtest("SQL injection in search query returns 0 results"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/search?q=test%27%20OR%201%3D1%20--' | jq '.projects | length'"
        )
        assert result.strip() == "0", f"Expected 0, got {result.strip()}"
        # Verify projects table is intact
        count = machine.succeed(
            "sudo -u circus psql -U circus -d circus -t -c 'SELECT COUNT(*) FROM projects'"
        )
        assert int(count.strip()) > 0, "Projects table seems damaged"

    with subtest("Path traversal in cache returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "'http://127.0.0.1:3000/nix-cache/nar/../../../etc/passwd.nar'"
        )
        # Should be 404 (not 200)
        assert code.strip() in ("400", "404"), f"Expected 400/404 for path traversal, got {code.strip()}"

    with subtest("Oversized request body returns 413"):
        # Generate a payload larger than 10MB (the default max_body_size)
        code = machine.succeed(
            "dd if=/dev/zero bs=1M count=12 2>/dev/null | "
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "--data-binary @-"
        )
        assert code.strip() == "413", f"Expected 413 for oversized body, got {code.strip()}"

    with subtest("NULL bytes in project name returns 400"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"null\\u0000byte\", \"repository_url\": \"https://example.com/null\"}'"
        )
        assert code.strip() == "400", f"Expected 400 for null bytes, got {code.strip()}"

    # Dashboard page smoke tests
    with subtest("All dashboard pages return 200"):
        pages = ["/", "/projects", "/evaluations", "/builds", "/queue", "/channels", "/admin", "/login"]
        for page in pages:
            code = machine.succeed(
                f"curl -s -o /dev/null -w '%{{http_code}}' http://127.0.0.1:3000{page}"
            )
            assert code.strip() == "200", f"Page {page} returned {code.strip()}, expected 200"
  '';
}
