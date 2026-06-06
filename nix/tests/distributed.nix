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
      environment.systemPackages = with pkgs; [curl jq openssl util-linux];

      environment.etc."circus/cache-key.sec" = {
        text = "circus-test-cache-1:C7h9wunIEh7kCa2Ylpa/omVaLewO7gQTb2LEPCPeJ0G6ZsLd2SaJZyt44z6nX5RanSkkrjM4xwapqVPneHY83A==";
        mode = "0400";
        user = "circus";
      };

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
          cache.enabled = true;
          signing = {
            enabled = true;
            key_file = "/etc/circus/cache-key.sec";
          };
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
                  builtins.hashString "sha256" "demo-agent-token-please-rotate"
                }"
              ];
              cache_substituter = "http://runner:3000/nix-cache";
              cache_public_key = "circus-test-cache-1:umbC3dkmiWcreOM+p1+UWp0pJK4zOMcGqalT53h2PNw=";
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
        runner.wait_until_succeeds("setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -c 'SELECT 1'", timeout=30)
        runner.wait_for_unit("circus-server.service")
        runner.wait_for_unit("circus-queue-runner.service")
        runner.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    with subtest("RPC listener is open"):
        runner.wait_for_open_port(8443)

    with subtest("Agent connects and registers"):
        agent.wait_for_unit("circus-agent.service")
        # Agent registers within a couple of heartbeats.
        runner.wait_until_succeeds(
            "setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND connected=TRUE\" | grep -qE '^ *1$'",
            timeout=30,
        )

    import json
    import re
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
            "setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND last_seen > NOW() - INTERVAL '15 seconds'\" | grep -qE '^ *1$'",
            timeout=20,
        )

    with subtest("Stopping the agent flips connected to FALSE"):
        agent.systemctl("stop circus-agent.service")
        runner.wait_until_succeeds(
            "setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND connected=FALSE\" | grep -qE '^ *1$'",
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
            "setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -tAc \"SELECT count(*) FROM builder_sessions WHERE name='agent-01' AND connected=TRUE\" | grep -qE '^ *1$'",
            timeout=30,
        )
        out = runner.succeed(
            f"curl -sf {auth} http://127.0.0.1:3000/api/v1/admin/builders/sessions/{agent_machine_id}"
        )
        row = json.loads(out)
        assert row.get("connected") is True, f"expected connected=True after restart, got: {row}"

    with subtest("Create project with a buildable flake"):
        runner.succeed("mkdir -p /var/lib/circus/test-repos")
        runner.succeed("git init --bare /var/lib/circus/test-repos/distributed-cache.git")
        runner.succeed("git config --global --add safe.directory /var/lib/circus/test-repos/distributed-cache.git")
        runner.succeed("mkdir -p /tmp/distributed-cache-work")
        runner.succeed("cd /tmp/distributed-cache-work && git init")
        runner.succeed("cd /tmp/distributed-cache-work && git config user.email 'test@circus' && git config user.name 'circus Test'")
        runner.succeed(
            "cat > /tmp/distributed-cache-work/flake.nix << 'FLAKE'\n"
            "{\n"
            '  description = "circus distributed cache test";\n'
            '  outputs = { self, ... }: {\n'
            '    packages.x86_64-linux.agent-cache-test = derivation {\n'
            '      name = "circus-agent-cache-test";\n'
            '      system = "x86_64-linux";\n'
            '      builder = "builtin:fetchurl";\n'
            '      url = "file://''${builtins.toFile "circus-agent-cache-test.txt" "agent-cache-test\\n"}";\n'
            '      outputHashMode = "flat";\n'
            '      outputHashAlgo = "sha256";\n'
            '      outputHash = "sha256-wq3ayny+lhFrJwgYNU6Jqb74vaFul5WqlbD7fCtCYRI=";\n'
            "    };\n"
            "  };\n"
            "}\n"
            "FLAKE\n"
        )
        runner.succeed("cd /tmp/distributed-cache-work && git add -A && git commit -m 'initial flake'")
        runner.succeed("cd /tmp/distributed-cache-work && git remote add origin /var/lib/circus/test-repos/distributed-cache.git")
        runner.succeed("cd /tmp/distributed-cache-work && git push origin HEAD:refs/heads/master")
        runner.succeed("chown -R circus:circus /var/lib/circus/test-repos")

        project_id = runner.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"distributed-cache\", \"repository_url\": \"file:///var/lib/circus/test-repos/distributed-cache.git\"}' "
            "| jq -r .id"
        ).strip()
        runner.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets "
            f"{auth} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"packages\", \"nix_expression\": \"packages\", \"flake_mode\": true, \"enabled\": true, \"check_interval\": 10}'"
        )

    with subtest("Distributed agent build is served by runner binary cache"):
        runner.wait_until_succeeds(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=agent-cache-test' "
            "| jq -e '.items[] | select(.status==\"succeeded\")'",
            timeout=120,
        )
        build = json.loads(runner.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=agent-cache-test' "
            "| jq -c '.items[] | select(.status==\"succeeded\")' | head -1"
        ))
        build_id = build["id"]
        output_path = build["build_output_path"]
        assert output_path.startswith("/nix/store/"), f"expected store output, got: {output_path}"

        hash_match = re.match(r"/nix/store/([a-z0-9]+)-", output_path)
        assert hash_match, f"could not extract store hash from: {output_path}"
        store_hash = hash_match.group(1)

        runner.succeed(
            "setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -tAc \""
            "SELECT count(*) FROM builds b JOIN builder_sessions s ON b.agent_machine_id = s.machine_id "
            f"WHERE b.id='{build_id}' AND s.name='agent-01' AND b.status='succeeded'\" "
            "| grep -qE '^ *1$'"
        )

        narinfo = runner.succeed(f"curl -sf http://127.0.0.1:3000/nix-cache/{store_hash}.narinfo")
        assert f"StorePath: {output_path}" in narinfo, f"narinfo should describe agent-built output: {narinfo}"
        assert "NarHash:" in narinfo, f"narinfo missing NarHash: {narinfo}"

        nar_url = next(
            line.split(": ", 1)[1]
            for line in narinfo.splitlines()
            if line.startswith("URL: ")
        )
        runner.succeed(f"curl -sf 'http://127.0.0.1:3000/nix-cache/{nar_url}' > /tmp/distributed-agent-output.nar")
        runner.succeed("test -s /tmp/distributed-agent-output.nar")
  '';
}
