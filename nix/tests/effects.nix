{
  testers,
  pkgs,
  self,
}: let
  effectsSource = pkgs.fetchFromGitHub {
    owner = "hercules-ci";
    repo = "hercules-ci-effects";
    rev = "41134a44913c4a32cdb03e5e82ac331af0ff3efc";
    hash = "sha256-4sTf6FyZLUG3UJ1VNAHBbUGvNoiIfpPaju6D7RsDQrY=";
  };
  testTls =
    pkgs.runCommand "circus-effects-test-tls" {
      nativeBuildInputs = [pkgs.openssl];
    } ''
      mkdir -p "$out"
      openssl req -x509 -newkey rsa:2048 -nodes \
        -keyout "$out/ca.key" \
        -out "$out/ca.crt" \
        -days 3650 \
        -subj "/CN=Circus Effects Test CA" \
        -addext "basicConstraints=critical,CA:TRUE" \
        -addext "keyUsage=critical,keyCertSign,cRLSign"
      openssl req -new -newkey rsa:2048 -nodes \
        -keyout "$out/runner.key" \
        -out "$out/runner.csr" \
        -subj "/CN=runner" \
        -addext "subjectAltName=DNS:runner" \
        -addext "extendedKeyUsage=serverAuth"
      openssl x509 -req \
        -in "$out/runner.csr" \
        -CA "$out/ca.crt" \
        -CAkey "$out/ca.key" \
        -CAcreateserial \
        -copy_extensions copy \
        -out "$out/runner.crt" \
        -days 3650
      rm "$out/ca.key" "$out/ca.srl" "$out/runner.csr"
    '';
  effectScript = ''
    test "$IN_HERCULES_CI_EFFECT" = true
    test "$HERCULES_CI_API_BASE_URL" = http://runner:3000
    secret="$(readSecretString deploy .token)"
    getStateFile deploy-state state.txt
    test ! -e state.txt
    printf deployed > state.txt
    putStateFile deploy-state state.txt
    getStateFile deploy-state roundtrip.txt
    test "$(cat roundtrip.txt)" = deployed
    curl --fail --silent --show-error \
      --data-binary "$secret:$HERCULES_CI_PROJECT_PATH" \
      http://runner:8001/effect
  '';
  testFlake = pkgs.writeText "flake.nix" ''
    {
      outputs = { self, ... }: let
        pkgs = import ${pkgs.path} { system = "x86_64-linux"; };
        effects = (import ${effectsSource} { inherit pkgs; }).effects;
        effectInput = pkgs.writeShellScriptBin "circus-effect-input" "exit 0";
      in {
        packages.x86_64-linux = {
          payload = derivation {
            name = "circus-effect-payload";
            system = "x86_64-linux";
            builder = "''${pkgs.bash}/bin/bash";
            args = [ "-euc" "printf ready > \"$out\"" ];
            requiredSystemFeatures = ["circus-effects-runner-only"];
          };

          deploy = effects.mkEffect {
            name = "circus-deploy";
            effectScript = ${builtins.toJSON effectScript};
            inputs = [effectInput];
            secretsMap.deploy = "deploy-token";
            requiredSystemFeatures = ["circus-effects-agent-only"];
          };
        };
      };
    }
  '';

  receiver = pkgs.writeText "effect-receiver.py" ''
    from http.server import BaseHTTPRequestHandler, HTTPServer

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            size = int(self.headers.get("content-length", "0"))
            with open("/tmp/effect-result", "wb") as output:
                output.write(self.rfile.read(size))
            self.send_response(204)
            self.end_headers()

        def log_message(self, *_args):
            pass

    HTTPServer(("0.0.0.0", 8001), Handler).serve_forever()
  '';
in
  testers.runNixOSTest {
    name = "circus-effects";

    nodes = {
      runner = {lib, ...}: {
        imports = [(import ../common/distributed-runner.nix {inherit self;})];
        virtualisation = {
          diskSize = 10 * 1000;
          memorySize = 2048;
          cores = 2;
        };
        environment.systemPackages = [pkgs.python3];
        system.extraDependencies = [
          pkgs.bash
          pkgs.cacert
          pkgs.curl.dev
          pkgs.jq.dev
          pkgs.stdenvNoCC
        ];
        nix.settings.system-features = lib.mkAfter ["circus-effects-runner-only"];
        networking.firewall.allowedTCPPorts = [8000 8001];
        services.circus.settings.queue_runner.rpc.api_base_url = "http://runner:3000";
        services.circus.settings.queue_runner.local_features = ["circus-effects-runner-only"];
        services.circus.settings.evaluator.restrict_eval = false;
        # Concurrent evix workers instantiating mkEffect's overlapping nixpkgs
        # graph intermittently fail with "path ... is not valid".
        services.circus.settings.evaluator.eval_workers = 1;
        services.circus.settings.server.allowed_url_schemes =
          lib.mkForce ["https" "http" "git" "file"];
        services.circus.settings.queue_runner.rpc.allow_plaintext =
          lib.mkForce false;
        services.circus.settings.queue_runner.rpc.tls = {
          cert_file = "${testTls}/runner.crt";
          key_file = "${testTls}/runner.key";
        };
      };

      agent = {lib, ...}: {
        imports = [(import ../common/distributed-agent.nix {inherit self;})];
        virtualisation = {
          diskSize = 10 * 1000;
          memorySize = 2048;
          cores = 2;
        };
        services.circus-agent.settings.agent.runner_url =
          lib.mkForce "circus+tls://runner:8443";
        services.circus-agent.settings.agent.tls.ca_file = "${testTls}/ca.crt";
        services.circus-agent.settings.agent.supported_features = [
          "circus-effects-agent-only"
        ];
        services.circus-agent.settings.agent.mandatory_features = [
          "circus-effects-agent-only"
        ];
        services.circus-agent.effectsSecretsFile = "/run/circus-effects/secrets.json";
        systemd.services.circus-effects-test-secret = {
          before = ["circus-agent.service"];
          requiredBy = ["circus-agent.service"];
          serviceConfig.Type = "oneshot";
          script = ''
            umask 077
            token="$(cat /proc/sys/kernel/random/uuid)"
            install -d -m 0700 -o circus-agent -g circus-agent /run/circus-effects
            ${pkgs.jq}/bin/jq -n --arg token "$token" '
              {
                "deploy-token": {
                  "kind": "Secret",
                  "data": { "token": $token },
                  "condition": {
                    "and": [
                      { "isOwner": "test-owner" },
                      { "isRepo": "test-flake" },
                      "isDefaultBranch"
                    ]
                  }
                }
              }
            ' > /run/circus-effects/secrets.json
            chown circus-agent:circus-agent /run/circus-effects/secrets.json
            chmod 0600 /run/circus-effects/secrets.json
          '';
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

        def wait_one_row(q):
            runner.wait_until_succeeds(psql(f"SELECT count(*) FROM {q}") + " | grep -qE '^ *1$'", timeout=180)

        def wait_any_row(q):
            runner.wait_until_succeeds(psql(f"SELECT EXISTS (SELECT 1 FROM {q})") + " | grep -q '^t$'", timeout=180)

        with subtest("Services and effect-capable agent are ready"):
            runner.wait_for_unit("circus-server.service")
            runner.wait_for_unit("circus-queue-runner.service")
            runner.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=60)
            runner.wait_for_open_port(8443)
            agent.wait_for_unit("circus-agent.service")
            wait_one_row("builder_sessions WHERE name='agent-01' AND connected")
            agent.succeed(
                "test \"$(stat -c %a /run/circus-effects/secrets.json)\" = 600",
                "test \"$(stat -c %a /run/circus-agent/effects-secrets.json)\" = 600",
                "test \"$(stat -c %a /run/circus-agent/circus-agent.toml)\" = 600",
            )

        with subtest("Publish effect flake and create trusted jobset"):
            runner.succeed(
                "mkdir -p /var/lib/circus/test-repos/test-owner",
                "git init --bare -q /var/lib/circus/test-repos/test-owner/test-flake.git",
                "git config --global --add safe.directory '*'",
                "git init -q /tmp/wc",
                "cp ${testFlake} /tmp/wc/flake.nix",
                "git -C /tmp/wc add -A",
                "git -C /tmp/wc -c user.email=circus@manic.systems -c user.name=circus commit -qm flake",
                "git -C /tmp/wc push -q /var/lib/circus/test-repos/test-owner/test-flake.git HEAD:refs/heads/master",
                "chown -R circus:circus /var/lib/circus/test-repos",
                "git daemon --reuseaddr --export-all --listen=0.0.0.0 --port=8000 --base-path=/var/lib/circus/test-repos /var/lib/circus/test-repos >/tmp/effect-git.log 2>&1 &",
                "python3 ${receiver} >/tmp/effect-receiver.log 2>&1 &",
            )
            runner.wait_for_open_port(8000)
            runner.wait_for_open_port(8001)
            project = runner.succeed(
                f"""curl -sf -X POST {api}/projects {auth} -H 'Content-Type: application/json' """
                """-d '{"name":"effects","repository_url":"git://runner:8000/test-owner/test-flake.git"}' | jq -r .id"""
            ).strip()
            runner.succeed(
                f"""curl -sf -X POST {api}/projects/{project}/jobsets {auth} -H 'Content-Type: application/json' """
                """-d '{"name":"packages","nix_expression":"packages","flake_mode":true,"enabled":true,"check_interval":10,"branch":"master"}'"""
            )

        with subtest("Effect waits for builds and resolves its local secret"):
            wait_one_row(
                "builds WHERE kind='effect' AND job_name='x86_64-linux.deploy'"
            )
            wait_any_row(
                "build_dependencies d "
                "JOIN builds effect ON effect.id=d.build_id AND effect.kind='effect' "
                "JOIN builds build ON build.id=d.dependency_build_id AND build.kind='build'"
            )
            wait_one_row(
                "builds WHERE kind='build' AND job_name NOT LIKE 'drv:%' "
                "AND status='succeeded'"
            )
            wait_one_row(
                "builds WHERE kind='effect' AND job_name='x86_64-linux.deploy' "
                "AND status='succeeded'"
            )
            runner.wait_until_succeeds(
                "grep -Eq '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}:runner/test-owner/test-flake$' /tmp/effect-result",
                timeout=180,
            )

        with subtest("Effect state files persist"):
            wait_one_row(
                "project_state_files WHERE name='deploy-state' "
                "AND convert_from(data, 'UTF8')='deployed'"
            )

        with subtest("Build and Effect use their intended venues"):
            wait_one_row(
                "builds WHERE kind='build' AND job_name='x86_64-linux.payload' "
                "AND status='succeeded' AND agent_machine_id IS NULL"
            )
            wait_one_row(
                "builds b JOIN builder_sessions s ON b.agent_machine_id=s.machine_id "
                "WHERE b.kind='effect' AND b.job_name='x86_64-linux.deploy' "
                "AND s.name='agent-01'"
            )
      '';
  }
