{
  self,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkDefault;
  circus-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
in {
  # Common machine configuration for circus integration tests that run as
  # nspawn containers rather than QEMU VMs. Identical to vm-common.nix but
  # without the virtualisation.* options that only exist in qemu-vm.nix.
  config = {
    programs.git.enable = true;
    security.sudo.enable = true;

    environment.systemPackages = with pkgs; [
      nix
      nix-eval-jobs
      zstd
      curl
      jq
      openssl
      python3
      util-linux
    ];

    nix = {
      settings.experimental-features = ["nix-command" "flakes" "auto-allocate-uids"];
      settings.substituters = lib.mkForce [];
    };

    networking.firewall.allowedTCPPorts = [3000];

    services.circus = {
      enable = true;

      package = mkDefault circus-packages.circus-server;
      evaluatorPackage = mkDefault circus-packages.circus-evaluator;
      queueRunnerPackage = mkDefault circus-packages.circus-queue-runner;
      migratePackage = mkDefault circus-packages.circus-migrate-cli;

      server.enable = true;
      evaluator.enable = true;
      queueRunner.enable = true;

      settings = {
        database.url = "postgresql:///circus?host=/run/postgresql";
        server = {
          host = "127.0.0.1";
          port = 3000;
          cors_permissive = false;
          allowed_url_schemes = ["https" "http" "git" "ssh" "file"];
          # Functional tests read the API anonymously: a documented public
          # read-only posture. Dashboard page_access keeps its secure defaults
          # and is validated by auth-rbac.nix.
          require_api_key_for_reads = false;
        };

        gc.enabled = false;
        logs.log_dir = "/var/lib/circus/logs";
        cache.enabled = true;
        signing.enabled = false;

        tracing = {
          level = "info";
          format = "compact";
          show_targets = true;
          show_timestamps = true;
        };

        evaluator = {
          poll_interval = 5;
          work_dir = "/var/lib/circus/evaluator";
          nix_timeout = 60;
          strict_errors = true;
        };

        queue_runner = {
          poll_interval = 3;
          work_dir = "/var/lib/circus/queue-runner";
          strict_errors = true;
        };
      };

      declarative.apiKeys = [
        {
          name = "bootstrap-admin";
          key = "circus_bootstrap_key";
          role = "admin";
        }
      ];

      declarative.projects = [
        {
          name = "declarative-project";
          repositoryUrl = "https://github.com/test/declarative";
          description = "Test declarative project";
          jobsets = [
            {
              name = "packages";
              nixExpression = "packages";
              flakeMode = true;
              enabled = true;
              checkInterval = 3600;
              state = "disabled";
            }
          ];
        }
      ];
    };
  };
}
