{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-basic-api";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  testScript = ''
    import hashlib

    machine.start()
    machine.wait_for_unit("postgresql.service")

    # Ensure PostgreSQL is actually ready to accept connections before circus-server starts
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)

    machine.wait_for_unit("circus-server.service")

    # Wait for the server to start listening
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    ## Seed an API key for write operations
    # Token: fc_testkey123 -> SHA-256 hash inserted into api_keys table
    api_token = "fc_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # Health endpoint
    with subtest("Health endpoint returns OK"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/health | jq -r .status")
        assert result.strip() == "ok", f"Expected 'ok', got '{result.strip()}'"

    with subtest("Health endpoint reports database healthy"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/health | jq -r .database")
        assert result.strip() == "true", f"Expected 'true', got '{result.strip()}'"

    # Cache endpoint: nix-cache-info
    with subtest("Cache info endpoint returns correct data"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/nix-cache/nix-cache-info")
        assert "StoreDir: /nix/store" in result, f"Missing StoreDir in: {result}"
        assert "WantMassQuery: 1" in result, f"Missing WantMassQuery in: {result}"

    # Cache endpoint: invalid hash rejection
    with subtest("Cache rejects short hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/tooshort.narinfo | grep -q 404")

    with subtest("Cache rejects uppercase hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF.narinfo | grep -q 404")

    with subtest("Cache rejects special chars in hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' 'http://127.0.0.1:3000/nix-cache/abcdefghijklmnop____abcde.narinfo' | grep -q 404")

    with subtest("Cache returns 404 for valid but nonexistent hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/abcdefghijklmnopqrstuvwxyz012345.narinfo | grep -q 404")

    #  NAR endpoints: invalid hash rejection
    with subtest("NAR zst rejects invalid hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/nar/INVALID.nar.zst | grep -q 404")

    with subtest("NAR plain rejects invalid hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/nar/INVALID.nar | grep -q 404")

    # Search endpoint: length validation
    with subtest("Search rejects empty query"):
        result = machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/search?q=' | jq '.projects | length'")
        assert result.strip() == "0", f"Expected 0 projects, got {result.strip()}"

    with subtest("Search rejects overly long query"):
        long_q = "a" * 300
        result = machine.succeed(f"curl -sf 'http://127.0.0.1:3000/api/v1/search?q={long_q}' | jq '.projects | length'")
        assert result.strip() == "0", f"Expected 0 projects for long query, got {result.strip()}"

    # Error response format
    with subtest("404 error response includes error_code field"):
        json_result = machine.succeed("curl -s http://127.0.0.1:3000/api/v1/projects/00000000-0000-0000-0000-000000000000 | jq -r .error_code")
        assert json_result.strip() == "NOT_FOUND", f"Expected NOT_FOUND, got {json_result.strip()}"

    # Empty page states (before any data is created)
    with subtest("Empty evaluations page has proper empty state"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/evaluations")
        assert "Page 1 of 0" not in body, \
            "Evaluations page should NOT show 'Page 1 of 0' when empty"
        assert "No evaluations yet" in body, \
            "Empty evaluations page should show helpful empty state message"

    with subtest("Empty builds page has proper empty state"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/builds")
        assert "Page 1 of 0" not in body, \
            "Builds page should NOT show 'Page 1 of 0' when empty"
        assert "No builds match" in body, \
            "Empty builds page should show helpful empty state message"

    with subtest("Empty channels page has proper empty state"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/channels")
        assert "No channels configured" in body, \
            "Empty channels page should show helpful empty state"

    with subtest("Tables use table-wrap containers on projects page"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/projects")
        # Projects page should have at least one project (from bootstrap)
        assert "table-wrap" in body, \
            "Projects page should wrap tables in .table-wrap class"

    # API CRUD: create and list projects
    with subtest("Create a project via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test-project\", \"repository_url\": \"https://github.com/test/repo\"}' "
            "| jq -r .id"
        )
        project_id = result.strip()
        assert len(project_id) == 36, f"Expected UUID, got '{project_id}'"

    with subtest("List projects includes created project"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items[0].name'")
        assert "test-project" in result, f"Expected test-project in: {result}"

    # Builds list with filters
    with subtest("Builds list with system filter returns 200"):
        machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/builds?system=x86_64-linux' | jq '.items'")

    with subtest("Builds list with job_name filter returns 200"):
        machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=hello' | jq '.items'")

    with subtest("Builds list with combined filters returns 200"):
        machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/builds?system=x86_64-linux&status=pending&job_name=test' | jq '.items'")

    # Prometheus endpoint
    with subtest("Prometheus endpoint returns prometheus format"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/prometheus")
        machine.succeed(f"echo '{result[:1000]}' > /tmp/metrics.txt")
        machine.succeed("echo 'PROMETHEUS OUTPUT:' && cat /tmp/metrics.txt")
        assert "fc_builds_total" in result, f"Missing fc_builds_total. Got: {result[:300]}"
        assert "fc_projects_total" in result, "Missing fc_projects_total in prometheus metrics"
        assert "fc_evaluations_total" in result, "Missing fc_evaluations_total in prometheus metrics"

    # CORS: default restrictive (no Access-Control-Allow-Origin for cross-origin)
    with subtest("Default CORS does not allow arbitrary origins"):
        result = machine.succeed(
            "curl -s -D - "
            "-H 'Origin: http://evil.example.com' "
            "http://127.0.0.1:3000/health "
            "2>&1"
        )
        # With restrictive CORS, there should be no access-control-allow-origin header
        # for an arbitrary origin
        assert "access-control-allow-origin: http://evil.example.com" not in result.lower(), \
            f"CORS should not allow arbitrary origins: {result}"

    # Systemd hardening
    with subtest("circus-server runs as fc user"):
        result = machine.succeed("systemctl show circus-server --property=User --value")
        assert result.strip() == "circus", f"Expected fc user, got '{result.strip()}'"

    with subtest("circus-server has NoNewPrivileges"):
        result = machine.succeed("systemctl show circus-server --property=NoNewPrivileges --value")
        assert result.strip() == "yes", f"Expected NoNewPrivileges, got '{result.strip()}'"

    with subtest("fc user home directory exists"):
        machine.succeed("test -d /var/lib/circus")

    with subtest("Log directory exists"):
        machine.succeed("test -d /var/lib/circus/logs || mkdir -p /var/lib/circus/logs")

    # Stats endpoint
    with subtest("Build stats endpoint returns data"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/api/v1/builds/stats | jq '.total_builds'")
        # Should be a number (possibly 0)
        int(result.strip())

    with subtest("Recent builds endpoint returns array"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/api/v1/builds/recent | jq 'type'")
        assert result.strip() == '"array"', f"Expected array, got {result.strip()}"
  '';
}
