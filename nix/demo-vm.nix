{
  self,
  pkgs,
  lib,
}: let
  inherit (lib.modules) mkForce;
  hostPkgs = pkgs;
  hostSystem = hostPkgs.stdenv.hostPlatform.system;
  guestSystem = builtins.replaceStrings ["darwin"] ["linux"] hostSystem;
  circusPkgs = self.packages.${guestSystem};

  adminPasswordFile = builtins.toFile "admin-password" "AdminPassword123!";
  demoPasswordFile = builtins.toFile "demo-password" "DemoPassword123!";

  module = {
    modulesPath,
    pkgs,
    ...
  }: {
    imports = [
      (modulesPath + "/virtualisation/qemu-vm.nix")

      self.nixosModules.circus
      ./common/vm.nix

      {config._module.args = {inherit self;};}
    ];

    nixpkgs.hostPlatform = guestSystem;

    ## VM hardware
    # As it turns out 2gb and 2 cores is not enough.
    virtualisation = {
      host.pkgs = hostPkgs;
      memorySize = mkForce 4096;
      cores = mkForce 4;
    };

    ## Seed an admin API key on first boot
    # Token: circus_demo_admin_key, SHA-256 hash inserted into api_keys
    # A read-only key is also seeded for testing RBAC.
    systemd.services.circus-seed-keys = {
      description = "Seed demo API keys";
      after = ["circus-server.service"];
      requires = ["circus-server.service"];
      wantedBy = ["multi-user.target"];
      path = with pkgs; [postgresql curl];
      script = ''
        # Wait for server to be ready
        for i in $(seq 1 30); do
          if curl -sf http://127.0.0.1:3000/health >/dev/null 2>&1; then
            break
          fi
          sleep 1
        done

        # Admin key: circus_demo_admin_key
        ADMIN_HASH="$(echo -n 'circus_demo_admin_key' | sha256sum | cut -d' ' -f1)"
        psql -U circus -d circus -c "INSERT INTO api_keys (name, key_hash, role) VALUES ('demo-admin', '$ADMIN_HASH', 'admin') ON CONFLICT DO NOTHING" 2>/dev/null || true

        # Read-only key: circus_demo_readonly_key
        RO_HASH="$(echo -n 'circus_demo_readonly_key' | sha256sum | cut -d' ' -f1)"
        psql -U circus -d circus -c "INSERT INTO api_keys (name, key_hash, role) VALUES ('demo-readonly', '$RO_HASH', 'read-only') ON CONFLICT DO NOTHING" 2>/dev/null || true
      '';

      serviceConfig = {
        RemainAfterExit = true;
        Type = "oneshot";
        User = "circus";
        Group = "circus";
      };
    };
    services = {
      circus = {
        enable = true;

        server.enable = true;
        evaluator.enable = true;
        queueRunner.enable = true;

        settings = {
          database.url = "postgresql:///circus?host=/run/postgresql";
          gc.enabled = false;
          logs.log_dir = "/var/lib/circus/logs";
          cache.enabled = true;
          signing.enabled = false;
          server = {
            # Bind to all interfaces so port forwarding works
            host = mkForce "0.0.0.0";
            port = 3000;
            cors_permissive = mkForce true;
          };

          declarative.users = [
            {
              username = "admin";
              email = "admin@circus.local";
              password_file = toString adminPasswordFile;
              role = "admin";
            }
            {
              username = "demo";
              email = "demo@circus.local";
              role = "read-only";
              password_file = toString demoPasswordFile;
            }
          ];
        };
      };

      getty = {
        autologinUser = "root";
        greetingLine = ''
          Dashboard:     http://localhost:3000
          Health:        http://localhost:3000/health
          API base:      http://localhost:3000/api/v1

          Web login:     admin / AdminPassword123! (admin)
                         demo / DemoPassword123! (read-only)

          Admin API key: circus_demo_admin_key
          Read-only key: circus_demo_readonly_key

          Login at <http://localhost:3000/login> using
          the credentials or the API key provided above.
        '';
      };
    };

    # Useful tools inside the VM
    environment.systemPackages = with pkgs; [
      curl
      jq
      btop
      zstd

      # Demo VMs should be able to use the admin CLI  and CLI should be
      # individually testable. As there is no separate toggle for this
      # we can make it explicit here.
      circusPkgs.circus-cli
    ];

    networking = {
      # Misc VM settings
      hostName = "circus-demo";
      firewall.allowedTCPPorts = [3000];
    };

    system.stateVersion = "26.11";
  };

  nixos = import "${self.inputs.nixpkgs}/nixos" {
    configuration = module;
    system = null;
  };
in
  pkgs.writeShellApplication {
    name = "run-circus-demo-vm";
    text = "exec ${nixos.config.system.build.vm}/bin/run-circus-demo-vm";
  }
