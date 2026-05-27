{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-api-crud";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];

    config._module.args = {inherit self;};
  };

  # API CRUD tests: dashboard content, project/jobset/evaluation/build/channel/builder
  # CRUD, admin endpoints, pagination, search
  testScript = ''
    import hashlib
    import json
    import re

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

    # Create initial project for tests
    result = machine.succeed(
        "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
        f"{auth_header} "
        "-H 'Content-Type: application/json' "
        "-d '{\"name\": \"test-project\", \"repository_url\": \"https://github.com/test/repo\"}' "
        "| jq -r .id"
    )
    project_id = result.strip()

    #  Dashboard content verification
    with subtest("Home page contains Dashboard heading"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "Dashboard" in body, "Home page missing 'Dashboard' heading"

    with subtest("Home page contains stats grid"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "stat-card" in body, "Home page missing stats grid"
        assert "Completed" in body, "Home page missing 'Completed' stat"

    with subtest("Home page shows project overview table"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        # We created projects earlier, they should appear
        assert "test-project" in body, "Home page should list test-project in overview"

    with subtest("Projects page contains created projects"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/projects")
        assert "test-project" in body, "Projects page should list test-project"

    with subtest("Projects page returns HTML content type"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/projects | grep -i content-type"
        )
        assert "text/html" in ct.lower(), f"Expected text/html, got: {ct}"

    with subtest("Admin page shows system status"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/admin")
        assert "Administration" in body, "Admin page missing heading"
        assert "System Status" in body, "Admin page missing system status section"
        assert "Remote Builders" in body, "Admin page missing remote builders section"

    with subtest("Queue page renders"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/queue")
        assert "Queue" in body or "Pending" in body or "Running" in body, \
            "Queue page missing expected content"

    with subtest("Channels page renders"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/channels")
        # Page should render even if empty
        assert "Channel" in body or "channel" in body, "Channels page missing expected content"

    with subtest("Builds page renders with filter params"):
        body = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/builds?status=pending&system=x86_64-linux'"
        )
        assert "Build" in body or "build" in body, "Builds page missing expected content"

    with subtest("Evaluations page renders"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/evaluations")
        assert "Evaluation" in body or "evaluation" in body, "Evaluations page missing expected content"

    with subtest("Login page contains form"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/login")
        assert "api_key" in body or "API" in body, "Login page missing API key input"
        assert "<form" in body.lower(), "Login page missing form element"

    # Dashboard page for specific entities
    with subtest("Project detail page renders for existing project"):
        body = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/project/{project_id}"
        )
        assert "test-project" in body, "Project detail page should show project name"

    with subtest("Project detail page with invalid UUID returns graceful error"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/project/00000000-0000-0000-0000-000000000000"
        )
        # Should return 200 with "not found" message or similar, not crash
        assert code.strip() == "200", f"Expected 200 for missing project detail, got {code.strip()}"

    # Project update via PUT
    with subtest("Update project description via PUT"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{project_id} "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"Updated description\"}'"
        )
        assert code.strip() == "200", f"Expected 200 for PUT project, got {code.strip()}"

    with subtest("Updated project reflects new description"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id} | jq -r .description"
        )
        assert result.strip() == "Updated description", f"Expected updated description, got: {result.strip()}"

    with subtest("Update project with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{project_id} "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"Hacked\"}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only PUT, got {code.strip()}"

    # Jobset CRUD
    with subtest("Create jobset for test-project"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"main\", \"nix_expression\": \"packages\"}' "
            "| jq -r .id"
        )
        test_jobset_id = result.strip()
        assert len(test_jobset_id) == 36, f"Expected UUID for jobset, got '{test_jobset_id}'"

    with subtest("List jobsets for project includes new jobset"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq '.items | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 jobset, got {result.strip()}"

    with subtest("Jobset detail page renders"):
        body = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/jobset/{test_jobset_id}"
        )
        assert "main" in body, "Jobset detail page should show jobset name"

    # Evaluation trigger and lifecycle
    with subtest("Trigger evaluation via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/evaluations/trigger "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"jobset_id\": \"{test_jobset_id}\", \"commit_hash\": \"abcdef1234567890abcdef1234567890abcdef12\"}}' "
            "| jq -r .id"
        )
        test_eval_id = result.strip()
        assert len(test_eval_id) == 36, f"Expected UUID for evaluation, got '{test_eval_id}'"

    with subtest("Get evaluation by ID"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/evaluations/{test_eval_id} | jq -r .status"
        )
        assert result.strip().lower() == "pending", f"Expected pending status, got: {result.strip()}"

    with subtest("List evaluations includes triggered one"):
        result = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={test_jobset_id}' | jq '.items | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 evaluation, got {result.strip()}"

    with subtest("Evaluation detail dashboard page renders"):
        body = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/evaluation/{test_eval_id}"
        )
        assert "abcdef123456" in body, "Evaluation page should show commit hash prefix"

    with subtest("Trigger evaluation with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/evaluations/trigger "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"jobset_id\": \"{test_jobset_id}\", \"commit_hash\": \"0000000000000000000000000000000000000000\"}}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only eval trigger, got {code.strip()}"

    # Build lifecycle (restart, bump)
    # Create a build via SQL since builds are normally created by the evaluator
    with subtest("Create test build via SQL"):
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee', '{test_eval_id}', 'hello', '/nix/store/fake.drv', 'failed', 'x86_64-linux', 5, NOW())"
            "\""
        )
        test_build_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"

    with subtest("Get build by ID"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{test_build_id} | jq -r .status"
        )
        assert result.strip().lower() == "failed", f"Expected failed, got: {result.strip()}"

    with subtest("Restart failed build"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/builds/{test_build_id}/restart "
            f"{auth_header} "
            "| jq -r .status"
        )
        assert result.strip().lower() == "pending", f"Expected pending status for restarted build, got: {result.strip()}"

    with subtest("Restart with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/builds/{test_build_id}/restart "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only restart, got {code.strip()}"

    # Stop the queue runner so it cannot claim the build before we bump it
    machine.systemctl("stop circus-queue-runner.service")

    # Create a pending build to test bump
    with subtest("Create pending build for bump test"):
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('bbbbbbbb-cccc-dddd-eeee-ffffffffffff', '{test_eval_id}', 'world', '/nix/store/fake2.drv', 'pending', 'x86_64-linux', 5, NOW())"
            "\""
        )
        bump_build_id = "bbbbbbbb-cccc-dddd-eeee-ffffffffffff"

    with subtest("Bump build priority"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/builds/{bump_build_id}/bump "
            f"{auth_header} "
            "| jq -r .priority"
        )
        assert int(result.strip()) == 15, f"Expected priority 15 (5+10), got: {result.strip()}"

    with subtest("Bump with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/builds/{bump_build_id}/bump "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only bump, got {code.strip()}"

    with subtest("Cancel build"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/builds/{bump_build_id}/cancel "
            f"{auth_header} "
            "| jq '.[0].status'"
        )
        assert "cancelled" in result.strip().lower(), f"Expected cancelled, got: {result.strip()}"

    machine.systemctl("start circus-queue-runner.service")

    # Evaluation comparison
    with subtest("Trigger second evaluation for comparison"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/evaluations/trigger "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"jobset_id\": \"{test_jobset_id}\", \"commit_hash\": \"deadbeef1234567890abcdef1234567890abcdef\"}}' "
            "| jq -r .id"
        )
        second_eval_id = result.strip()
        # Add a build to the second evaluation
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('cccccccc-dddd-eeee-ffff-aaaaaaaaaaaa', '{second_eval_id}', 'hello', '/nix/store/changed.drv', 'pending', 'x86_64-linux', 5, NOW())"
            "\""
        )
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('dddddddd-eeee-ffff-aaaa-bbbbbbbbbbbb', '{second_eval_id}', 'new-pkg', '/nix/store/new.drv', 'pending', 'x86_64-linux', 5, NOW())"
            "\""
        )

    with subtest("Compare evaluations shows diff"):
        result = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations/{test_eval_id}/compare?to={second_eval_id}'"
        )
        data = json.loads(result)
        # hello changed derivation, world was removed, new-pkg was added
        assert len(data["changed_jobs"]) >= 1, f"Expected at least 1 changed job, got {data['changed_jobs']}"
        assert len(data["new_jobs"]) >= 1, f"Expected at least 1 new job, got {data['new_jobs']}"
        assert any(j["job_name"] == "new-pkg" for j in data["new_jobs"]), "new-pkg should be in new_jobs"

    # Channel CRUD lifecycle
    with subtest("Create channel via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/channels "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"project_id\": \"{project_id}\", \"name\": \"stable\", \"jobset_id\": \"{test_jobset_id}\"}}' "
            "| jq -r .id"
        )
        test_channel_id = result.strip()
        assert len(test_channel_id) == 36, f"Expected UUID for channel, got '{test_channel_id}'"

    with subtest("List channels includes new channel"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/channels | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 channel, got {result.strip()}"

    with subtest("Get channel by ID"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/channels/{test_channel_id} | jq -r .name"
        )
        assert result.strip() == "stable", f"Expected 'stable', got: {result.strip()}"

    with subtest("List project channels"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/channels | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 project channel, got {result.strip()}"

    with subtest("Promote channel to evaluation"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/channels/{test_channel_id}/promote/{test_eval_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for channel promote, got {code.strip()}"

    with subtest("Channel promote with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/channels/{test_channel_id}/promote/{test_eval_id} "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only promote, got {code.strip()}"

    with subtest("Create channel with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/channels "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"project_id\": \"{project_id}\", \"name\": \"nightly\", \"jobset_id\": \"{test_jobset_id}\"}}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only channel create, got {code.strip()}"

    with subtest("Delete channel"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/channels/{test_channel_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for channel delete, got {code.strip()}"

    # Remote builder CRUD lifecycle
    with subtest("List remote builders"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/admin/builders | jq 'length'"
        )
        # We created one earlier in auth tests
        assert int(result.strip()) >= 0, f"Expected >= 0 builders, got {result.strip()}"

    # Create a builder for testing
    machine.succeed(
        "curl -sf -X POST http://127.0.0.1:3000/api/v1/admin/builders "
        f"{auth_header} "
        "-H 'Content-Type: application/json' "
        "-d '{\"name\": \"test-builder\", \"ssh_uri\": \"ssh://nix@builder\", \"systems\": [\"x86_64-linux\"], \"max_jobs\": 2}'"
    )

    with subtest("Get remote builder by ID"):
        # Get the first builder's ID
        builder_id = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/admin/builders | jq -r '.[0].id'"
        ).strip()
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} | jq -r .name"
        )
        assert result.strip() == "test-builder", f"Expected 'test-builder', got: {result.strip()}"

    with subtest("Update remote builder (disable)"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"enabled\": false}'"
        )
        assert code.strip() == "200", f"Expected 200 for builder update, got {code.strip()}"

    with subtest("Updated builder is disabled"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} | jq -r .enabled"
        )
        assert result.strip() == "false", f"Expected false, got: {result.strip()}"

    with subtest("Update builder with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"enabled\": true}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only builder update, got {code.strip()}"

    with subtest("Delete remote builder with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only builder delete, got {code.strip()}"

    with subtest("Delete remote builder with admin key"):
        # First clear the builder_id from builds that reference it
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c "
            f"\"UPDATE builds SET builder_id = NULL WHERE builder_id = '{builder_id}'\""
        )
        # Now delete the builder
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for builder delete, got {code.strip()}"

    # Admin system status endpoint
    with subtest("System status endpoint requires admin"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/admin/system "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only system status, got {code.strip()}"

    with subtest("System status endpoint returns data with admin key"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/admin/system "
            f"{auth_header} "
            "| jq .projects_count"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 project in system status, got {result.strip()}"

    # API key listing
    with subtest("List API keys requires admin"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/api-keys "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only API key list, got {code.strip()}"

    with subtest("List API keys returns array with admin key"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/api-keys "
            f"{auth_header} "
            "| jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 API key, got {result.strip()}"

    # Badge endpoints
    with subtest("Badge endpoint returns SVG for unknown project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/job/nonexistent/main/hello/shield"
        )
        # Should return 404 or error since project doesn't exist
        assert code.strip() in ("404", "500"), f"Expected 404/500 for unknown badge, got {code.strip()}"

    with subtest("Badge endpoint returns SVG for existing project"):
        # Create a badge-compatible project name lookup
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/job/test-project/main/hello/shield"
        )
        # Should return 200 with SVG (even if no builds, shows "not found" badge)
        assert code.strip() == "200", f"Expected 200 for badge, got {code.strip()}"

    with subtest("Badge returns SVG content type"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null "
            "http://127.0.0.1:3000/api/v1/job/test-project/main/hello/shield "
            "| grep -i content-type"
        )
        assert "image/svg+xml" in ct.lower(), f"Expected SVG content type, got: {ct}"

    with subtest("Latest build endpoint for unknown project returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/job/nonexistent/main/hello/latest"
        )
        assert code.strip() in ("404", "500"), f"Expected 404/500 for latest build, got {code.strip()}"

    # Pagination tests
    # Re-verify server is healthy before pagination tests
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=15)

    with subtest("Projects pagination with limit and offset"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/projects?limit=1&offset=0' | jq '.items | length'"
        )
        assert int(result.strip()) == 1, f"Expected 1 project with limit=1, got {result.strip()}"

    with subtest("Projects pagination returns total count"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/projects?limit=1&offset=0' | jq '.total'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 total projects, got {result.strip()}"

    with subtest("Builds pagination with limit"):
        result = machine.succeed(
            "curl -s 'http://127.0.0.1:3000/api/v1/builds?limit=5'"
        )
        data = json.loads(result)
        assert "limit" in data, f"Expected paginated response with 'limit' field, got: {result[:300]}"
        assert data["limit"] == 5, f"Expected limit=5, got {data['limit']}"

    with subtest("Evaluations pagination with limit"):
        result = machine.succeed(
            "curl -s 'http://127.0.0.1:3000/api/v1/evaluations?limit=2'"
        )
        data = json.loads(result)
        assert "limit" in data, f"Expected paginated response with 'limit' field, got: {result[:300]}"
        assert data["limit"] == 2, f"Expected limit=2, got {data['limit']}"

    # Build sub-resources
    with subtest("Build steps endpoint returns empty array for nonexistent build"):
        result = machine.succeed(
            "curl -sf "
            "http://127.0.0.1:3000/api/v1/builds/00000000-0000-0000-0000-000000000000/steps"
            " | jq 'length'"
        )
        assert int(result.strip()) == 0, f"Expected empty steps array, got {result.strip()}"

    with subtest("Build products endpoint returns empty array for nonexistent build"):
        result = machine.succeed(
            "curl -sf "
            "http://127.0.0.1:3000/api/v1/builds/00000000-0000-0000-0000-000000000000/products"
            " | jq 'length'"
        )
        assert int(result.strip()) == 0, f"Expected empty products array, got {result.strip()}"

    with subtest("Build log endpoint for nonexistent build returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/builds/00000000-0000-0000-0000-000000000000/log"
        )
        assert code.strip() == "404", f"Expected 404 for nonexistent build log, got {code.strip()}"

    # Search functionality
    with subtest("Search returns matching projects"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/search?q=test-project' | jq '.projects | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 matching project, got {result.strip()}"

    with subtest("Search returns empty for nonsense query"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/search?q=zzzznonexistent99999' | jq '.projects | length'"
        )
        assert result.strip() == "0", f"Expected 0, got {result.strip()}"

    # Content-Type verification for API endpoints
    with subtest("API endpoints return application/json"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/api/v1/projects | grep -i content-type"
        )
        assert "application/json" in ct.lower(), f"Expected application/json, got: {ct}"

    with subtest("Health endpoint returns application/json"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/health | grep -i content-type"
        )
        assert "application/json" in ct.lower(), f"Expected application/json, got: {ct}"

    with subtest("Metrics endpoint returns text/plain"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/metrics | grep -i content-type"
        )
        assert "text/plain" in ct.lower() or "text/" in ct.lower(), f"Expected text content type for metrics, got: {ct}"

    # Session/Cookie auth for dashboard
    with subtest("Login with valid API key sets session cookie"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null "
            "-X POST http://127.0.0.1:3000/login "
            f"-d 'api_key={api_token}'"
        )
        assert "circus_session=" in result, f"Expected circus_session cookie in response: {result}"
        assert "HttpOnly" in result, "Expected HttpOnly flag on session cookie"

    with subtest("Login with invalid API key shows error"):
        body = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/login "
            "-d 'api_key=circus_invalid_key'"
        )
        assert "Invalid" in body or "invalid" in body or "error" in body.lower(), \
            f"Expected error message for invalid login: {body[:200]}"

    with subtest("Login with empty API key shows error"):
        body = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/login "
            "-d 'api_key='"
        )
        assert "required" in body.lower() or "error" in body.lower() or "Invalid" in body, \
            f"Expected error message for empty login: {body[:200]}"

    with subtest("Session cookie grants admin access on dashboard"):
        # Login and capture cookie
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
                f"curl -sf -H 'Cookie: circus_session={session_val}' http://127.0.0.1:3000/admin"
            )
            # Admin page with session should show API Keys section and admin controls
            assert "API Keys" in body, "Admin page with session should show API Keys section"

    with subtest("Logout clears session cookie"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null -X POST http://127.0.0.1:3000/logout"
        )
        assert "Max-Age=0" in result or "max-age=0" in result.lower(), \
            "Logout should set Max-Age=0 to clear cookie"

    # RBAC with create-projects role
    cp_token = "circus_createprojects_key"
    cp_hash = hashlib.sha256(cp_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('creator', '{cp_hash}', 'create-projects')\""
    )
    cp_header = f"-H 'Authorization: Bearer {cp_token}'"

    with subtest("create-projects role can create project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{cp_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"creator-project\", \"repository_url\": \"https://example.com/creator\"}'"
        )
        assert code.strip() == "200", f"Expected 200 for create-projects role, got {code.strip()}"

    with subtest("create-projects role cannot delete project"):
        # Get the new project ID
        cp_project_id = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"creator-project\") | .id'"
        ).strip()
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{cp_project_id} "
            f"{cp_header}"
        )
        assert code.strip() == "403", f"Expected 403 for create-projects role DELETE, got {code.strip()}"

    with subtest("create-projects role cannot update project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{cp_project_id} "
            f"{cp_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"hacked\"}'"
        )
        assert code.strip() == "403", f"Expected 403 for create-projects PUT, got {code.strip()}"

    with subtest("create-projects role cannot access admin endpoints"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/admin/system "
            f"{cp_header}"
        )
        assert code.strip() == "403", f"Expected 403 for create-projects system status, got {code.strip()}"

    # Additional security tests
    with subtest("DELETE project without auth returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{project_id}"
        )
        assert code.strip() == "401", f"Expected 401 for unauthenticated DELETE, got {code.strip()}"

    with subtest("PUT project without auth returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{project_id} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"hacked\"}'"
        )
        assert code.strip() == "401", f"Expected 401 for unauthenticated PUT, got {code.strip()}"

    with subtest("POST channel without auth returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/channels "
            "-H 'Content-Type: application/json' "
            "-d '{\"project_id\": \"00000000-0000-0000-0000-000000000000\", \"name\": \"x\", \"jobset_id\": \"00000000-0000-0000-0000-000000000000\"}'"
        )
        assert code.strip() == "401", f"Expected 401 for unauthenticated channel create, got {code.strip()}"

    with subtest("API returns JSON error body for 404"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects/00000000-0000-0000-0000-000000000001 2>&1 || "
            "curl -s http://127.0.0.1:3000/api/v1/projects/00000000-0000-0000-0000-000000000001"
        )
        parsed = json.loads(result)
        assert "error" in parsed or "error_code" in parsed, f"Expected JSON error body, got: {result}"

    with subtest("Nonexistent API route returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/nonexistent"
        )
        # Axum returns 404 for unmatched routes
        assert code.strip() in ("404", "405"), f"Expected 404/405 for nonexistent route, got {code.strip()}"

    with subtest("HEAD request to health returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' -I http://127.0.0.1:3000/health"
        )
        assert code.strip() == "200", f"Expected 200 for HEAD /health, got {code.strip()}"

    with subtest("OPTIONS request returns valid response"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X OPTIONS http://127.0.0.1:3000/api/v1/projects"
        )
        # Axum may return 200, 204, or 405 depending on CORS configuration
        assert code.strip() in ("200", "204", "405"), f"Expected 200/204/405 for OPTIONS, got {code.strip()}"
  '';
}
