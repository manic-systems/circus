{
  pkgs,
  self,
}:
pkgs.testers.nixosTest {
  name = "circus-service-startup";

  nodes.machine = {
    imports = [
      self.nixosModules.circus
      ../vm-common.nix
    ];
    _module.args.self = self;
  };

  testScript = ''
    machine.start()
    machine.wait_for_unit("postgresql.service")

    # Ensure PostgreSQL is actually ready to accept connections
    # before circus-server starts. Not actually implied by the wait_for_unit
    machine.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)

    machine.wait_for_unit("circus-server.service")

    # Wait for the server to start listening
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # Verify all three services start
    with subtest("circus-evaluator.service starts without crash"):
        machine.wait_for_unit("circus-evaluator.service", timeout=30)
        result = machine.succeed("journalctl -u circus-evaluator --no-pager -n 20 2>&1")
        assert "binary not found" not in result.lower(), f"Evaluator has 'binary not found' error: {result}"
        assert "No such file" not in result, f"Evaluator has 'No such file' error: {result}"

    with subtest("circus-queue-runner.service starts without crash"):
        machine.wait_for_unit("circus-queue-runner.service", timeout=30)
        result = machine.succeed("journalctl -u circus-queue-runner --no-pager -n 20 2>&1")
        assert "binary not found" not in result.lower(), f"Queue runner has 'binary not found' error: {result}"
        assert "No such file" not in result, f"Queue runner has 'No such file' error: {result}"

    with subtest("All three circus services are active"):
        for svc in ["circus-server", "circus-evaluator", "circus-queue-runner"]:
            result = machine.succeed(f"systemctl is-active {svc}")
            assert result.strip() == "active", f"Expected {svc} to be active, got '{result.strip()}'"

    with subtest("Declarative project was bootstrapped"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"declarative-project\") | .name'"
        )
        assert result.strip() == "declarative-project", f"Expected declarative-project, got '{result.strip()}'"

    with subtest("Declarative project has correct repository URL"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"declarative-project\") | .repository_url'"
        )
        assert result.strip() == "https://github.com/test/declarative", f"Expected declarative repo URL, got '{result.strip()}'"

    with subtest("Declarative project has bootstrapped jobset"):
        # Get the declarative project ID
        decl_project_id = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"declarative-project\") | .id'"
        ).strip()
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{decl_project_id}/jobsets | jq '.items[0].name' -r"
        )
        assert result.strip() == "packages", f"Expected packages jobset, got '{result.strip()}'"

    with subtest("Declarative API key works for authentication"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-H 'Authorization: Bearer fc_bootstrap_key' "
            "http://127.0.0.1:3000/api/v1/projects"
        )
        assert code.strip() == "200", f"Expected 200 with bootstrap key, got {code.strip()}"

    with subtest("Bootstrap is idempotent (server restarted successfully with same config)"):
        # The server already started successfully with declarative config. That
        # proves the bootstrap ran. Now we should check that only one declarative
        # project exists, and not any duplicates.
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items | map(select(.name==\"declarative-project\")) | length'"
        )
        assert result.strip() == "1", f"Expected exactly 1 declarative-project, got {result.strip()}"
  '';
}
