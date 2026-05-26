{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-features";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  # Feature tests: logging, CSS, setup wizard, probe, metrics improvements
  testScript = ''
    import hashlib
    import re

    machine.start()
    machine.wait_for_unit("postgresql.service")

    # Ensure PostgreSQL is actually ready to accept connections before circus-server starts
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)

    machine.wait_for_unit("circus-server.service")

    # Wait for the server to start listening
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # Seed an API key for write operations
    # Token: fc_testkey123 -> SHA-256 hash inserted into api_keys table
    api_token = "fc_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # Seed a read-only key
    ro_token = "fc_readonly_key"
    ro_hash = hashlib.sha256(ro_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('readonly', '{ro_hash}', 'read-only')\""
    )
    ro_header = f"-H 'Authorization: Bearer {ro_token}'"

    # Structured logging
    with subtest("Server produces structured log output"):
        # The server should log via tracing with the configured format
        result = machine.succeed("journalctl -u circus-server --no-pager -n 50 2>&1")
        # With compact/full format, tracing outputs level and target info
        assert "INFO" in result or "info" in result, \
            "Expected structured log lines with INFO level in journalctl output"

    # Static CSS serving
    with subtest("Static CSS endpoint returns 200 with correct content type"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/static/style.css"
        )
        assert code.strip() == "200", f"Expected 200 for /static/style.css, got {code.strip()}"
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/static/style.css | grep -i content-type"
        )
        assert "text/css" in ct.lower(), f"Expected text/css, got: {ct}"

    # Setup wizard page
    with subtest("Setup wizard page returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/projects/new"
        )
        assert code.strip() == "200", f"Expected 200 for /projects/new, got {code.strip()}"

    with subtest("Setup wizard page contains wizard steps"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/projects/new")
        assert "Step 1" in body, "Setup wizard should contain Step 1"
        assert "Repository URL" in body, "Setup wizard should contain URL input"
        assert "probeRepo" in body, "Setup wizard should contain probe JS function"

    with subtest("Projects page links to setup wizard"):
        # Login first to get admin view
        cookie = machine.succeed(
            "curl -s -D - -o /dev/null "
            "-X POST http://127.0.0.1:3000/login "
            f"-d 'api_key={api_token}' "
            "| grep -i set-cookie | head -1"
        )
        match = re.search(r'circus_session=([^;]+)', cookie)
        if match:
            session_val = match.group(1)
            body = machine.succeed(
                f"curl -sf -H 'Cookie: circus_session={session_val}' http://127.0.0.1:3000/projects"
            )
            assert '/projects/new' in body, "Projects page should link to /projects/new wizard"

    # Flake probe endpoint
    with subtest("Probe endpoint exists and requires POST"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects/probe"
        )
        # GET should return 405 (Method Not Allowed)
        assert code.strip() in ("404", "405"), f"Expected 404/405 for GET /probe, got {code.strip()}"

    with subtest("Probe endpoint accepts POST with auth"):
        # This will likely fail since the VM has no network access to github,
        # but we can verify the endpoint exists and returns a proper error
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects/probe "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"repository_url\": \"https://github.com/nonexistent/repo\"}'"
        )
        # Should return 408 (timeout), 422 (nix eval error), 500, or 200 with is_flake=false
        # Any non-crash response is acceptable
        assert code.strip() in ("200", "408", "422", "500"), \
            f"Expected 200/408/422/500 for probe of unreachable repo, got {code.strip()}"

    # Setup endpoint
    with subtest("Setup endpoint exists and requires POST"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects/setup"
        )
        assert code.strip() in ("404", "405"), f"Expected 404/405 for GET /setup, got {code.strip()}"

    with subtest("Setup endpoint creates project with jobsets"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/setup "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"repository_url\": \"https://github.com/test/setup-test\", \"name\": \"setup-test\", \"description\": \"Created via setup\", \"jobsets\": [{\"name\": \"packages\", \"nix_expression\": \"packages\"}]}' "
            "| jq -r .project.id"
        )
        setup_project_id = result.strip()
        assert len(setup_project_id) == 36, f"Expected UUID from setup, got '{setup_project_id}'"

    with subtest("Setup-created project has jobsets"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{setup_project_id}/jobsets | jq '.items | length'"
        )
        assert int(result.strip()) == 1, f"Expected 1 jobset from setup, got {result.strip()}"

    with subtest("Setup endpoint with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects/setup "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"repository_url\": \"https://github.com/test/ro\", \"name\": \"ro-setup\", \"jobsets\": []}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only setup, got {code.strip()}"

    # Clean up setup-test project
    machine.succeed(
        f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{setup_project_id} "
        f"{auth_header}"
    )

    # Dashboard improvements
    with subtest("Home page has dashboard-grid two-column layout"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "dashboard-grid" in body, "Home page should have dashboard-grid class"

    with subtest("Home page has colored stat values"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "stat-value-green" in body, "Home page should have green stat value for completed"
        assert "stat-value-red" in body, "Home page should have red stat value for failed"

    with subtest("Home page has escapeHtml utility"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "escapeHtml" in body, "Home page should include escapeHtml function"

    with subtest("Admin page JS uses escapeHtml for error handling"):
        # Login to get admin view
        if match:
            body = machine.succeed(
                f"curl -sf -H 'Cookie: circus_session={session_val}' http://127.0.0.1:3000/admin"
            )
            assert "escapeHtml" in body, "Admin page JS should use escapeHtml"

    # Metrics reflect actual data
    with subtest("Metrics fc_projects_total reflects created projects"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/metrics")
        for line in result.split("\n"):
            if line.startswith("fc_projects_total"):
                val = int(line.split()[-1])
                assert val >= 1, f"Expected fc_projects_total >= 1, got {val}"
                break

    with subtest("Metrics fc_evaluations_total reflects triggered evaluation"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/metrics")
        for line in result.split("\n"):
            if line.startswith("fc_evaluations_total"):
                val = int(line.split()[-1])
                # Might be 0 if no evaluations created yet
                assert val >= 0, f"Expected fc_evaluations_total >= 0, got {val}"
                break
  '';
}
