# Topology:
#
#   runner: postgresql + circus-server + circus-evaluator + circus-queue-runner
#           with [queue_runner.rpc] enabled (plain TCP, bearer token)
#   agent:  circus-agent connecting to runner:8443
#
{
  testers,
  self,
}:
testers.runNixOSTest {
  name = "circus-distributed";

  nodes = {
    runner = {
      pkgs,
      lib,
      ...
    }: let
      circus-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
    in {
      imports = [self.nixosModules.circus];
      _module.args.self = self;

      programs.git.enable = true;
      security.sudo.enable = true;
      environment.systemPackages = with pkgs; [curl jq openssl];

      nix.settings.experimental-features = ["nix-command" "flakes"];
      nix.settings.substituters = lib.mkForce [];

      networking.firewall.allowedTCPPorts = [3000 8443];

      services.postgresql = {
        enable = true;
        ensureDatabases = ["circus"];
        ensureUsers = [
          {
            name = "circus";
            ensureDBOwnership = true;
          }
        ];
      };

      services.circus = {
        enable = true;
        package = circus-packages.circus-server;
        evaluatorPackage = circus-packages.circus-evaluator;
        queueRunnerPackage = circus-packages.circus-queue-runner;
        migratePackage = circus-packages.circus-migrate-cli;

        server.enable = true;
        evaluator.enable = true;
        queueRunner.enable = true;

        settings = {
          database.url = "postgresql:///circus?host=/run/postgresql";
          server = {
            host = "0.0.0.0";
            port = 3000;
            cors_permissive = false;
            allowed_url_schemes = ["https" "http" "file"];
          };
          gc.enabled = false;
          logs.log_dir = "/var/lib/circus/logs";
          cache.enabled = false;
          signing.enabled = false;
          tracing = {
            level = "info";
            format = "compact";
          };
          queue_runner = {
            poll_interval = 3;
            work_dir = "/var/lib/circus/queue-runner";
            strict_errors = false;
            rpc = {
              bind = "0.0.0.0:8443";
              max_connections = 64;
              heartbeat_ttl_secs = 30;
              auth_tokens = [
                "${
                  builtins.hashString "sha256" "demo-agent-token-please-rotate-uwu"
                }"
              ];
            };
          };
        };

        declarative.apiKeys = [
          {
            name = "bootstrap-admin";
            key = "circus_bootstrap_key";
            role = "admin";
          }
        ];
      };
    };

    agent = {
      pkgs,
      lib,
      ...
    }: let
      circus-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
    in {
      imports = [self.nixosModules.circus-agent];
      _module.args.self = self;

      environment.systemPackages = with pkgs; [nix curl jq];
      nix.settings.experimental-features = ["nix-command" "flakes"];
      nix.settings.substituters = lib.mkForce [];

      environment.etc."circus-agent/token".text = "demo-agent-token-please-rotate";

      services.circus-agent = {
        enable = true;
        package = circus-packages.circus-agent;
        authTokenFile = "/etc/circus-agent/token";
        settings.agent = {
          name = "agent-01";
          runner_url = "circus://runner:8443";
          systems = [pkgs.stdenv.hostPlatform.system];
          max_jobs = 2;
          speed_factor = 1.0;
          heartbeat_interval_secs = 3;
          reconnect_delay_secs = 2;
        };
      };
    };
  };

  testScript = ''
    start_all()

    with subtest("Runner services come up"):
        runner.wait_for_unit("postgresql.service")
        runner.wait_until_succeeds("sudo -u circus psql -U circus -d circus -c 'SELECT 1'", timeout=30)
        runner.wait_for_unit("circus-server.service")
        runner.wait_for_unit("circus-queue-runner.service")
        runner.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    with subtest("RPC listener is open"):
        runner.wait_for_open_port(8443)

    with subtest("Agent connects and registers"):
        agent.wait_for_unit("circus-agent.service")
        # Agent registers within a couple of heartbeats.
        runner.wait_until_succeeds(
            "sudo -u circus psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND connected=TRUE\" | grep -qE '^ *1$'",
            timeout=30,
        )

    import json
    auth = "-H 'Authorization: Bearer circus_bootstrap_key'"

    with subtest("Admin /connected endpoint lists the live agent"):
        out = runner.succeed(
            f"curl -sf {auth} http://127.0.0.1:3000/api/v1/admin/builders/sessions/connected"
        )
        data = json.loads(out)
        assert any(s.get("name") == "agent-01" for s in data), \
            f"agent-01 missing from connected list: {out}"
        agent_machine_id = next(s["machine_id"] for s in data if s["name"] == "agent-01")

    with subtest("Admin /sessions endpoint shows the same row in the full history"):
        out = runner.succeed(
            f"curl -sf {auth} http://127.0.0.1:3000/api/v1/admin/builders/sessions"
        )
        data = json.loads(out)
        row = next((s for s in data if s.get("name") == "agent-01"), None)
        assert row is not None, f"agent-01 missing from full list: {out}"
        assert row["connected"] is True, f"expected connected=True in full list, got: {row}"

    with subtest("Admin /sessions/{machine_id} returns the single row"):
        out = runner.succeed(
            f"curl -sf {auth} http://127.0.0.1:3000/api/v1/admin/builders/sessions/{agent_machine_id}"
        )
        row = json.loads(out)
        assert row.get("name") == "agent-01", f"wrong name in single row: {row}"
        assert row.get("connected") is True, f"expected connected=True, got: {row}"

    with subtest("Heartbeat keeps last_seen fresh"):
        runner.wait_until_succeeds(
            "sudo -u circus psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND last_seen > NOW() - INTERVAL '15 seconds'\" | grep -qE '^ *1$'",
            timeout=20,
        )

    with subtest("Stopping the agent flips connected to FALSE"):
        agent.systemctl("stop circus-agent.service")
        runner.wait_until_succeeds(
            "sudo -u circus psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND connected=FALSE\" | grep -qE '^ *1$'",
            timeout=30,
        )

    with subtest("Single-row endpoint reflects the disconnect"):
        out = runner.succeed(
            f"curl -sf {auth} http://127.0.0.1:3000/api/v1/admin/builders/sessions/{agent_machine_id}"
        )
        row = json.loads(out)
        assert row.get("connected") is False, f"expected connected=False after stop, got: {row}"

    with subtest("Restarting the agent reconnects"):
        agent.systemctl("start circus-agent.service")
        runner.wait_until_succeeds(
            "sudo -u circus psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND connected=TRUE\" | grep -qE '^ *1$'",
            timeout=30,
        )
        out = runner.succeed(
            f"curl -sf {auth} http://127.0.0.1:3000/api/v1/admin/builders/sessions/{agent_machine_id}"
        )
        row = json.loads(out)
        assert row.get("connected") is True, f"expected connected=True after restart, got: {row}"
  '';
}
