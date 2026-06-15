# Test the runner and an agent on a SEPARATE store. The agent has no
# substituters or cache config of its own, so it can only build the dispatched
# drv by substituting its closure from the cache the runner advertises.
{
  testers,
  pkgs,
  self,
}: let
  # Trivial buildable flake: a FOD fetch of an inline file.
  testFlake = pkgs.writeText "flake.nix" ''
    {
      outputs = { self, ... }: {
        packages.x86_64-linux.hello = derivation {
          name = "circus-test-hello";
          system = "x86_64-linux";
          builder = "builtin:fetchurl";
          url = "file://''${builtins.toFile "hello.txt" "hello\n"}";
          outputHashMode = "flat";
          outputHashAlgo = "sha256";
          outputHash = "sha256-WJG1tSLV3whtD/CxEPvZ0hu0/HFjrzTQgoai6Eb2vgM=";
        };
      };
    }
  '';
in
  testers.runNixOSTest {
    name = "circus-agent-dispatch";

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

        virtualisation = {
          diskSize = 10 * 1000;
          memorySize = 2048;
          cores = 2;
        };

        programs.git.enable = true;
        security.sudo.enable = true;
        environment.systemPackages = with pkgs; [
          curl
          jq
          openssl
          util-linux
        ];

        # Throwaway signing key
        environment.etc."circus/cache-key.sec" = {
          text = "circus-test-cache-1:C7h9wunIEh7kCa2Ylpa/omVaLewO7gQTb2LEPCPeJ0G6ZsLd2SaJZyt44z6nX5RanSkkrjM4xwapqVPneHY83A==";
          mode = "0400";
          user = "circus";
        };

        nix.settings.experimental-features = [
          "nix-command"
          "flakes"
        ];
        nix.settings.substituters = lib.mkForce [];

        networking.firewall.allowedTCPPorts = [
          3000
          8443
        ];

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
              allowed_url_schemes = [
                "https"
                "http"
                "file"
              ];
            };
            gc.enabled = false;
            logs.log_dir = "/var/lib/circus/logs";
            # Serve the local store so the agent can substitute drv closures
            # (drvs and sources are content-addressed and served unsigned).
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
                allow_plaintext = true;
                max_connections = 64;
                heartbeat_ttl_secs = 30;
                auth_tokens = [
                  "${builtins.hashString "sha256" "demo-agent-token-please-rotate"}"
                ];
                # Advertised to agents in the assignment.
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

        virtualisation = {
          diskSize = 10 * 1000;
          memorySize = 2048;
          cores = 2;
        };

        environment.systemPackages = with pkgs; [
          nix
          curl
          jq
        ];
        nix.settings.experimental-features = [
          "nix-command"
          "flakes"
        ];
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

    testScript =
      /*
      py
      */
      ''
        start_all()

        auth = "-H 'Authorization: Bearer circus_bootstrap_key'"
        api = "http://127.0.0.1:3000/api/v1"

        def psql(q):
            return f"""setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -tAc "{q}" """

        # Wait until a count(*) query returns exactly 1.
        def wait_row(q):
            runner.wait_until_succeeds(psql(f"SELECT count(*) FROM {q}") + " | grep -qE '^ *1$'", timeout=180)

        with subtest("Services up, agent registered"):
            runner.wait_for_unit("circus-server.service")
            runner.wait_for_unit("circus-queue-runner.service")
            runner.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=60)
            runner.wait_for_open_port(8443)
            agent.wait_for_unit("circus-agent.service")
            # Register before the jobset exists, so the build dispatches to a live agent.
            wait_row("builder_sessions WHERE name='agent-01' AND connected")

        with subtest("Publish flake, create jobset"):
            runner.succeed(
                "mkdir -p /var/lib/circus/test-repos",
                "git init --bare -q /var/lib/circus/test-repos/test-flake.git",
                "git config --global --add safe.directory '*'",
                "git init -q /tmp/wc",
                "cp ${testFlake} /tmp/wc/flake.nix",
                "git -C /tmp/wc add -A",
                "git -C /tmp/wc -c user.email=circus@manic.systems -c user.name=circus commit -qm flake",
                "git -C /tmp/wc push -q /var/lib/circus/test-repos/test-flake.git HEAD:refs/heads/master",
                "chown -R circus:circus /var/lib/circus/test-repos",
            )
            project = runner.succeed(
                f"""curl -sf -X POST {api}/projects {auth} -H 'Content-Type: application/json' """
                """-d '{"name":"t","repository_url":"file:///var/lib/circus/test-repos/test-flake.git"}' | jq -r .id"""
            ).strip()
            runner.succeed(
                f"""curl -sf -X POST {api}/projects/{project}/jobsets {auth} -H 'Content-Type: application/json' """
                """-d '{"name":"packages","nix_expression":"packages","flake_mode":true,"enabled":true,"check_interval":10}'"""
            )

        with subtest("Build dispatches to and succeeds on the agent"):
            # builds_succeeded only rises when the agent realises the drv via the cache, otherwise it would time out.
            wait_row("builder_sessions WHERE name='agent-01' AND builds_succeeded >= 1")
            assert runner.succeed(psql("SELECT builds_failed FROM builder_sessions WHERE name='agent-01'")).strip() == "0"

        with subtest("Build is attributed to the agent, not local"):
            # agent_machine_id must point at agent-01's session row
            wait_row(
                "builds b JOIN builder_sessions s ON b.agent_machine_id = s.machine_id "
                "WHERE s.name='agent-01'"
            )
      '';
  }
