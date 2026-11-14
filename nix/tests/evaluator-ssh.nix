{
  testers,
  self,
  lib,
}: let
  machine = encrypted: {pkgs, ...}: {
    imports = [
      self.nixosModules.circus
      ../common/vm.nix
    ];
    _module.args.self = self;
    virtualisation = {
      # The forge only needs loopback
      vlans = [];
      forwardPorts = lib.mkForce [];
    };

    services.circus = {
      queueRunner.enable = lib.mkForce false;
      settings.outbound.allow_internal = ["127.0.0.1"];
      evaluator.ssh = {
        privateKeyFile =
          if encrypted
          then null
          else "/run/circus-ssh/client-key";
        encryptedPrivateKeyFile =
          if encrypted
          then "/run/circus-ssh/client-key.cred"
          else null;
        knownHostsFile = "/run/circus-ssh/known_hosts";
      };
    };
    # Install the runtime-only keys before starting the real module service.
    systemd.services.circus-evaluator.wantedBy = lib.mkForce [];
    environment.systemPackages = [pkgs.openssh];

    users = {
      groups.git = {};
      users.git = {
        isSystemUser = true;
        group = "git";
        home = "/srv/git";
        createHome = true;
        shell = pkgs.bashInteractive;
        hashedPassword = "";
        openssh.authorizedKeys.keyFiles = [];
      };
    };

    services.openssh = {
      enable = true;
      startWhenNeeded = false;
      settings = {
        PasswordAuthentication = false;
        KbdInteractiveAuthentication = false;
        AllowUsers = ["git"];
        AuthorizedKeysFile = "/srv/git/authorized_keys";
        LogLevel = "VERBOSE";
      };
    };
  };
in
  testers.runNixOSTest ({config, ...}: let
    inherit (config.node.pkgs.stdenv.hostPlatform) system;
  in {
    name = "circus-evaluator-ssh";
    requiredFeatures.kvm = false;
    nodes = {
      plaintext = machine false;
      encrypted = machine true;
    };

    testScript = ''
      import hashlib
      import json
      import shlex

      token = "circus_evaluator_ssh_test"
      auth = "-H " + shlex.quote("Authorization: Bearer " + token)

      def api(node, path, body=None):
          command = "curl -sS --fail-with-body " + auth
          if body is not None:
              command += " -H 'Content-Type: application/json' -d " + shlex.quote(json.dumps(body))
          return json.loads(node.succeed(command + " " + shlex.quote("http://127.0.0.1:3000/api/v1/" + path)))

      def namespace(node, unit):
          pid = node.succeed("systemctl show -p MainPID --value " + unit).strip()
          assert int(pid) > 0, (unit, pid)
          return "nsenter --target " + pid + " --mount -- "

      for node, encrypted_key in [(plaintext, False), (encrypted, True)]:
          node.start()
          node.wait_for_unit("sshd.service")
          node.wait_for_unit("postgresql.service")
          node.wait_for_unit("circus-server.service")
          node.succeed("nix store info --store daemon || { journalctl -u nix-daemon.service --no-pager; exit 1; }")
          node.wait_for_unit("nix-daemon.service")
          # The evaluator is deliberately stopped and the queue runner disabled.
          node.wait_until_succeeds("curl -s http://127.0.0.1:3000/health | jq -e '.database == true'", timeout=60)
          node.succeed(
              "setpriv --reuid=circus --regid=circus --init-groups psql -U circus -d circus -c "
              + shlex.quote("INSERT INTO api_keys (name, key_hash, role) VALUES ('ssh-test', '"
                            + hashlib.sha256(token.encode()).hexdigest() + "', 'admin')")
          )

          with subtest(node.name + ": ephemeral SSH-only repository"):
              node.succeed("install -d -m 0700 /run/circus-ssh")
              node.succeed("ssh-keygen -q -t ed25519 -N \"\" -f /run/circus-ssh/client-key")
              node.succeed("ssh-keygen -q -t ed25519 -N \"\" -f /run/circus-ssh/wrong-host-key")
              node.succeed("install -m 0644 /run/circus-ssh/client-key.pub /srv/git/authorized_keys")
              node.succeed("awk '{print \"127.0.0.1 \" $1 \" \" $2}' /run/circus-ssh/wrong-host-key.pub > /run/circus-ssh/known_hosts")
              if encrypted_key:
                  node.succeed("systemd-creds setup")
                  node.succeed(
                      "systemd-creds encrypt --with-key=host --name=ssh-private-key "
                      "/run/circus-ssh/client-key /run/circus-ssh/client-key.cred"
                  )
                  node.succeed("rm /run/circus-ssh/client-key")

              node.succeed("install -d -m 0700 /srv/git/private.git")
              node.succeed("git init --bare --initial-branch=main /srv/git/private.git")
              node.succeed("mkdir /tmp/ssh-flake && git -C /tmp/ssh-flake init --initial-branch=main")
              # self.outPath makes a checkout/path-source substitution observably
              # different from Nix's canonical git+ssh source in the drv hash.
              flake = """
              {
                outputs = { self }: {
                  packages.${system}.ssh-test = derivation {
                    name = "circus-private-ssh";
                    system = "${system}";
                    builder = "/bin/sh";
                    args = [ "-c" "echo private-ssh > $out" ];
                    source = self.outPath;
                  };
                };
              }
              """
              node.succeed("printf %s " + shlex.quote(flake) + " > /tmp/ssh-flake/flake.nix")
              node.succeed(
                  "git -C /tmp/ssh-flake add flake.nix && "
                  "git -C /tmp/ssh-flake -c user.name=Test -c user.email=test@example.invalid commit -m initial && "
                  "git -C /tmp/ssh-flake push /srv/git/private.git HEAD:main && "
                  "chown -R git:git /srv/git/private.git"
              )
              revision = node.succeed("git -C /tmp/ssh-flake rev-parse HEAD").strip()
              node.succeed("rm -rf /tmp/ssh-flake")
              node.fail("setpriv --reuid=circus-evaluator --regid=circus-evaluator --init-groups git ls-remote /srv/git/private.git")

          project = api(node, "projects", {
              "name": "private-ssh", "repository_url": "ssh://git@127.0.0.1/srv/git/private.git"
          })
          jobset = api(node, "projects/" + project["id"] + "/jobsets", {
              "name": "packages", "nix_expression": "packages", "flake_mode": True,
              "enabled": True, "check_interval": 10, "branch": "main"
          })

          with subtest(node.name + ": mismatched host key rejects evaluator fetch"):
              node.succeed("systemctl start circus-evaluator.service")
              node.wait_for_unit("circus-evaluator.service")
              node.wait_until_succeeds(
                  "journalctl -u circus-evaluator.service --no-pager | grep -E 'Host key verification failed|REMOTE HOST IDENTIFICATION HAS CHANGED'",
                  timeout=60
              )
              evaluations = api(node, "evaluations?jobset_id=" + jobset["id"])
              assert evaluations["items"] == [], evaluations
              node.succeed("systemctl stop circus-evaluator.service")
              node.succeed("awk '{print \"127.0.0.1 \" $1 \" \" $2}' /etc/ssh/ssh_host_ed25519_key.pub > /run/circus-ssh/known_hosts")
              node.succeed("systemctl start circus-evaluator.service")
              node.wait_for_unit("circus-evaluator.service")

          with subtest(node.name + ": SSH config and credentials stay service-local"):
              evaluator_ns = namespace(node, "circus-evaluator.service")
              server_ns = namespace(node, "circus-server.service")
              pid = node.succeed("systemctl show -p MainPID --value circus-evaluator.service").strip()
              credentials = node.succeed("python3 -c " + shlex.quote(
                  "from pathlib import Path; "
                  "env = dict(item.split(b'=', 1) for item in "
                  "Path('/proc/" + pid + "/environ').read_bytes().split(b'\\0') if item); "
                  "print(env[b'CREDENTIALS_DIRECTORY'].decode())"
              )).strip()
              as_evaluator = (
                  "setpriv --reuid=circus-evaluator --regid=circus-evaluator --init-groups "
                  "env HOME=/var/lib/circus-evaluator "
              )
              node.succeed(evaluator_ns + as_evaluator + "test -r " + shlex.quote(credentials + "/ssh-private-key"))
              node.succeed(evaluator_ns + as_evaluator + "test -r " + shlex.quote(credentials + "/ssh-known-hosts"))
              node.fail(as_evaluator + "test -r /run/circus-ssh/known_hosts")
              node.fail(
                  server_ns + "setpriv --reuid=circus-server --regid=circus-server --init-groups "
                  + "test -r " + shlex.quote(credentials + "/ssh-private-key")
              )
              node.fail(
                  server_ns + "setpriv --reuid=circus-server --regid=circus-server --init-groups "
                  + "timeout 10 git ls-remote ssh://git@127.0.0.1/srv/git/private.git </dev/null"
              )
              node.fail(as_evaluator + "timeout 10 git ls-remote ssh://git@127.0.0.1/srv/git/private.git </dev/null")

          with subtest(node.name + ": evaluator completes canonical git+ssh evaluation"):
              node.wait_until_succeeds(
                  "curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id=" + jobset["id"]
                  + "' | jq -e '.items[] | select(.status == \"completed\")'",
                  timeout=120
              )
              completed = [e for e in api(node, "evaluations?jobset_id=" + jobset["id"])["items"]
                           if e["status"] == "completed"]
              assert completed[0]["commit_hash"] == revision, completed
              builds = api(node, "builds?job_name=ssh-test")["items"]
              assert len(builds) == 1, builds
              build = builds[0]
              assert build["evaluation_id"] == completed[0]["id"], build
              assert build["system"] == "${system}", build
              canonical = "git+ssh://git@127.0.0.1/srv/git/private.git?rev=" + revision
              expected_drv = node.succeed(
                  evaluator_ns + as_evaluator + "env CREDENTIALS_DIRECTORY=" + shlex.quote(credentials)
                  + " nix eval --raw --no-write-lock-file "
                  + shlex.quote(canonical + "#packages.${system}.ssh-test.drvPath")
              ).strip()
              assert build["drv_path"] == expected_drv, (build, expected_drv)
          node.shutdown()
    '';
  })
