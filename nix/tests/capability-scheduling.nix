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
            url = "http://runner:8000/fungible.txt";
            outputHashMode = "flat";
            outputHashAlgo = "sha256";
            outputHash = "sha256-9oWaD6E7O4521XC87GceZdWST/u9e+QFYSpMRRpFq6U=";
          };
          packages.x86_64-linux.kvmonly = derivation {
            name = "circus-test-kvmonly";
            system = "x86_64-linux";
            builder = "builtin:fetchurl";
            url = "http://runner:8000/kvmonly.txt";
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
  }: {lib, ...}: {
    imports = [
      (import ../common/distributed-agent.nix {
        inherit self name features speed;
        maxJobs = 1;
      })
    ];

    nix.settings.system-features = lib.mkForce (
      [
        "nixos-test"
        "benchmark"
        "big-parallel"
      ]
      ++ features
    );
  };
in
  testers.runNixOSTest {
    name = "circus-capability-scheduling";

    nodes = {
      runner = {pkgs, ...}: {
        imports = [(import ../common/distributed-runner.nix {inherit self;})];

        virtualisation.diskSize = 10000;

        environment.systemPackages = [pkgs.python3];
        networking.firewall.allowedTCPPorts = [8000];
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
            runner.succeed("systemctl kill --signal=SIGSTOP circus-queue-runner.service")

        with subtest("Publish flake, create jobset"):
            runner.succeed(
                "mkdir -p /var/lib/circus/test-fixtures",
                "printf 'fungible\\n' > /var/lib/circus/test-fixtures/fungible.txt",
                "printf 'kvmonly\\n' > /var/lib/circus/test-fixtures/kvmonly.txt",
                "python3 -m http.server 8000 --bind 0.0.0.0 --directory /var/lib/circus/test-fixtures >/tmp/circus-test-fixtures.log 2>&1 &",
                "mkdir -p /var/lib/circus/test-repos",
                "git init --bare -q /var/lib/circus/test-repos/test-flake.git",
                "git init -q /tmp/wc",
                "cp ${testFlake} /tmp/wc/flake.nix",
                "git -C /tmp/wc add -A",
                "git -C /tmp/wc -c user.email=circus@manic.systems -c user.name=circus commit -qm flake",
                "git -C /tmp/wc push -q /var/lib/circus/test-repos/test-flake.git HEAD:refs/heads/master",
                "chown -R circus:circus /var/lib/circus/test-repos",
            )
            runner.wait_for_open_port(8000)
            runner.succeed("curl -sf http://127.0.0.1:8000/fungible.txt")
            runner.succeed("curl -sf http://127.0.0.1:8000/kvmonly.txt")
            project = runner.succeed(
                f"""curl -sf -X POST {api}/projects {auth} -H 'Content-Type: application/json' """
                """-d '{"name":"t","repository_url":"file:///var/lib/circus/test-repos/test-flake.git"}' | jq -r .id"""
            ).strip()
            runner.succeed(
                f"""curl -sf -X POST {api}/projects/{project}/jobsets {auth} -H 'Content-Type: application/json' """
                """-d '{"name":"packages","nix_expression":"packages","flake_mode":true,"enabled":true,"check_interval":10}'"""
            )
            wait_row("builds WHERE job_name LIKE '%kvmonly' AND status='pending'")
            wait_row("builds WHERE job_name LIKE '%fungible' AND status='pending'")

        with subtest("Resume scheduling with both builds pending"):
            runner.succeed("systemctl kill --signal=SIGCONT circus-queue-runner.service")

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
