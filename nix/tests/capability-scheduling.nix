# A kvm-capable builder should be reserved for kvm work, not win fungible builds on load.
{
  testers,
  pkgs,
  self,
}: let
  testFlake =
    pkgs.writeText "flake.nix"
    /*
    nix
    */
    ''
      {
        outputs = { self, ... }: {
          packages.x86_64-linux.fungible = derivation {
            name = "circus-test-fungible";
            system = "x86_64-linux";
            builder = "builtin:fetchurl";
            url = "file://${builtins.toFile "fungible.txt" "fungible\n"}";
            outputHashMode = "flat";
            outputHashAlgo = "sha256";
            outputHash = "sha256-9oWaD6E7O4521XC87GceZdWST/u9e+QFYSpMRRpFq6U=";
          };
          packages.x86_64-linux.kvmonly = derivation {
            name = "circus-test-kvmonly";
            system = "x86_64-linux";
            builder = "builtin:fetchurl";
            url = "file://${builtins.toFile "kvmonly.txt" "kvmonly\n"}";
            outputHashMode = "flat";
            outputHashAlgo = "sha256";
            outputHash = "sha256-0pYYEKsxj4yuHDGPHknsT7d9BX86+g9xDOalDHiFoOw=";
            requiredSystemFeatures = [ "kvm" ];
          };
        };
      }
    '';

  mkAgent = {
    name,
    features,
    speed,
  }: {
    pkgs,
    lib,
    ...
  }: let
    circus-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
  in {
    imports = [self.nixosModules.circus-agent];
    _module.args.self = self;

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
    nix.settings.system-features = lib.mkForce (
      [
        "nixos-test"
        "benchmark"
        "big-parallel"
      ]
      ++ features
    );

    environment.etc."circus-agent/token".text = "demo-agent-token-please-rotate";

    services.circus-agent = {
      enable = true;
      package = circus-packages.circus-agent;
      authTokenFile = "/etc/circus-agent/token";
      settings.agent = {
        inherit name;
        runner_url = "circus://runner:8443";
        systems = [pkgs.stdenv.hostPlatform.system];
        supported_features = features;
        max_jobs = 1;
        speed_factor = speed;
        heartbeat_interval_secs = 3;
        reconnect_delay_secs = 2;
      };
    };
  };
in
  testers.runNixOSTest {
    name = "circus-capability-scheduling";

    containers = {
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
        environment.systemPackages = with pkgs; [
          curl
          jq
          openssl
          util-linux
        ];

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
                  "${builtins.hashString "sha256" "demo-agent-token-please-rotate"}"
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

      kvm = mkAgent {
        name = "agent-kvm";
        features = ["kvm"];
        speed = 2.0;
      };
      plain = mkAgent {
        name = "agent-plain";
        features = [];
        speed = 1.0;
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

        def wait_row(q):
            runner.wait_until_succeeds(psql(f"SELECT count(*) FROM {q}") + " | grep -qE '^ *1$'", timeout=240)

        with subtest("Services up, both agents registered"):
            runner.wait_for_unit("circus-server.service")
            runner.wait_for_unit("circus-queue-runner.service")
            runner.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=60)
            runner.wait_for_open_port(8443)
            kvm.wait_for_unit("circus-agent.service")
            plain.wait_for_unit("circus-agent.service")
            wait_row("builder_sessions WHERE name='agent-kvm' AND connected")
            wait_row("builder_sessions WHERE name='agent-plain' AND connected")

        with subtest("Publish flake, create jobset"):
            runner.succeed(
                "mkdir -p /var/lib/circus/test-repos",
                "git init --bare -q /var/lib/circus/test-repos/test-flake.git",
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

        with subtest("Both builds complete"):
            wait_row("builds WHERE job_name LIKE '%kvmonly' AND status='succeeded'")
            wait_row("builds WHERE job_name LIKE '%fungible' AND status='succeeded'")

        with subtest("kvm-only build ran on agent-kvm"):
            wait_row(
                "builds b JOIN builder_sessions s ON b.agent_machine_id = s.machine_id "
                "WHERE b.job_name LIKE '%kvmonly' AND s.name='agent-kvm'"
            )

        with subtest("fungible build was kept off agent-kvm"):
            attributed = runner.succeed(psql(
                "SELECT s.name FROM builds b JOIN builder_sessions s "
                "ON b.agent_machine_id = s.machine_id WHERE b.job_name LIKE '%fungible'"
            )).strip()
            assert attributed == "agent-plain", f"fungible should be preserved off the kvm agent, ran on: {attributed!r}"
      '';
  }
