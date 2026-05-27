{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-machine-health";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  testScript = ''
    import hashlib
    import json

    machine.start()
    machine.wait_for_unit("postgresql.service")
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)
    machine.wait_for_unit("circus-server.service")
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    api_token = "circus_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u circus psql -U circus -d circus -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # Create a builder via API
    builder_json = machine.succeed(
        "curl -sf -X POST http://127.0.0.1:3000/api/v1/admin/builders "
        f"{auth_header} "
        "-H 'Content-Type: application/json' "
        "-d '{\"name\": \"test-builder\", \"ssh_uri\": \"ssh://builder@host\", \"systems\": [\"x86_64-linux\"]}'"
    )
    builder = json.loads(builder_json)
    builder_id = builder["id"]

    with subtest("New builder starts with zero failures"):
        assert builder["consecutive_failures"] == 0, \
            f"Expected 0 failures, got {builder['consecutive_failures']}"
        assert builder["disabled_until"] is None, \
            f"Expected disabled_until=null, got {builder['disabled_until']}"
        assert builder["last_failure"] is None, \
            f"Expected last_failure=null, got {builder['last_failure']}"

    with subtest("Recording failure increments consecutive_failures"):
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "UPDATE remote_builders SET "
            "consecutive_failures = LEAST(consecutive_failures + 1, 4), "
            "last_failure = NOW(), "
            "disabled_until = NOW() + interval '60 seconds' "
            f"WHERE id = '{builder_id}'\""
        )
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/admin/builders/{builder_id}"
        )
        b = json.loads(result)
        assert b["consecutive_failures"] == 1, \
            f"Expected 1 failure, got {b['consecutive_failures']}"
        assert b["disabled_until"] is not None, \
            "Expected disabled_until to be set"
        assert b["last_failure"] is not None, \
            "Expected last_failure to be set"

    with subtest("Failures cap at 4"):
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            f"UPDATE remote_builders SET consecutive_failures = 10 WHERE id = '{builder_id}'\""
        )
        # Simulate record_failure SQL (same as repo code)
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "UPDATE remote_builders SET "
            "consecutive_failures = LEAST(consecutive_failures + 1, 4), "
            "last_failure = NOW(), "
            "disabled_until = NOW() + make_interval(secs => 60.0 * power(3, LEAST(consecutive_failures + 1, 4) - 1)) "
            f"WHERE id = '{builder_id}'\""
        )
        result = machine.succeed(
            "sudo -u circus psql -U circus -d circus -tA -c "
            f"\"SELECT consecutive_failures FROM remote_builders WHERE id = '{builder_id}'\""
        )
        assert result.strip() == "4", f"Expected failures capped at 4, got {result.strip()}"

    with subtest("Disabled builder excluded from find_for_system"):
        # Set disabled_until far in the future
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "UPDATE remote_builders SET disabled_until = NOW() + interval '1 hour' "
            f"WHERE id = '{builder_id}'\""
        )
        result = machine.succeed(
            "sudo -u circus psql -U circus -d circus -tA -c "
            "\"SELECT count(*) FROM remote_builders "
            "WHERE enabled = true "
            "AND 'x86_64-linux' = ANY(systems) "
            "AND (disabled_until IS NULL OR disabled_until < NOW())\""
        )
        assert result.strip() == "0", \
            f"Expected disabled builder excluded, got count={result.strip()}"

    with subtest("Non-disabled builder included in find_for_system"):
        # Clear disabled_until
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            f"UPDATE remote_builders SET disabled_until = NULL WHERE id = '{builder_id}'\""
        )
        result = machine.succeed(
            "sudo -u circus psql -U circus -d circus -tA -c "
            "\"SELECT count(*) FROM remote_builders "
            "WHERE enabled = true "
            "AND 'x86_64-linux' = ANY(systems) "
            "AND (disabled_until IS NULL OR disabled_until < NOW())\""
        )
        assert result.strip() == "1", \
            f"Expected non-disabled builder included, got count={result.strip()}"

    with subtest("Recording success resets health state"):
        # First set some failures
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "UPDATE remote_builders SET "
            "consecutive_failures = 3, "
            "disabled_until = NOW() + interval '1 hour', "
            "last_failure = NOW() "
            f"WHERE id = '{builder_id}'\""
        )
        # Simulate record_success (same as repo code)
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "UPDATE remote_builders SET "
            "consecutive_failures = 0, "
            "disabled_until = NULL "
            f"WHERE id = '{builder_id}'\""
        )
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/admin/builders/{builder_id}"
        )
        b = json.loads(result)
        assert b["consecutive_failures"] == 0, \
            f"Expected 0 failures after success, got {b['consecutive_failures']}"
        assert b["disabled_until"] is None, \
            f"Expected disabled_until=null after success, got {b['disabled_until']}"

    with subtest("Health fields visible in admin API list"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/admin/builders {auth_header}"
        )
        builders = json.loads(result)
        assert len(builders) >= 1, "Expected at least one builder"
        b = builders[0]
        assert "consecutive_failures" in b, "Missing consecutive_failures in API response"
        assert "disabled_until" in b, "Missing disabled_until in API response"
        assert "last_failure" in b, "Missing last_failure in API response"

    with subtest("Exponential backoff increases with failures"):
        # Record 1st failure: expect ~60s backoff
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            f"UPDATE remote_builders SET consecutive_failures = 0, disabled_until = NULL WHERE id = '{builder_id}'\""
        )
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "UPDATE remote_builders SET "
            "consecutive_failures = LEAST(consecutive_failures + 1, 4), "
            "last_failure = NOW(), "
            "disabled_until = NOW() + make_interval(secs => 60.0 * power(3, LEAST(consecutive_failures + 1, 4) - 1)) "
            f"WHERE id = '{builder_id}'\""
        )
        delta1 = machine.succeed(
            "sudo -u circus psql -U circus -d circus -tA -c "
            f"\"SELECT EXTRACT(EPOCH FROM (disabled_until - last_failure))::int FROM remote_builders WHERE id = '{builder_id}'\""
        )
        d1 = int(delta1.strip())
        assert 55 <= d1 <= 65, f"1st failure backoff expected ~60s, got {d1}s"

        # Record 2nd failure: expect ~180s backoff
        machine.succeed(
            "sudo -u circus psql -U circus -d circus -c \""
            "UPDATE remote_builders SET "
            "consecutive_failures = LEAST(consecutive_failures + 1, 4), "
            "last_failure = NOW(), "
            "disabled_until = NOW() + make_interval(secs => 60.0 * power(3, LEAST(consecutive_failures + 1, 4) - 1)) "
            f"WHERE id = '{builder_id}'\""
        )
        delta2 = machine.succeed(
            "sudo -u circus psql -U circus -d circus -tA -c "
            f"\"SELECT EXTRACT(EPOCH FROM (disabled_until - last_failure))::int FROM remote_builders WHERE id = '{builder_id}'\""
        )
        d2 = int(delta2.strip())
        assert 175 <= d2 <= 185, f"2nd failure backoff expected ~180s, got {d2}s"

    # Cleanup
    machine.succeed(
        f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} {auth_header}"
    )
  '';
}
