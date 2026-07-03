{
  self,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkForce;
in {
  documentation.enable = false;
  programs.git.enable = true;
  security.sudo.enable = true;

  environment.systemPackages = with pkgs; [
    nix
    zstd
    curl
    jq
    openssl
    python3
    util-linux
  ];

  nix = {
    settings.experimental-features = ["nix-command" "flakes" "auto-allocate-uids"];
    settings.substituters = mkForce [];
  };

  networking.firewall.allowedTCPPorts = [3000];

  services.circus = {
    enable = true;

    server.enable = true;
    evaluator.enable = true;
    queueRunner.enable = true;

    settings = {
      database.url = "postgresql:///circus?host=/run/postgresql";
      server = {
        host = "127.0.0.1";
        port = 3000;
        cors_permissive = false;

        # Allow file:// URLs in integration tests (no network, repos are local).
        allowed_url_schemes = ["https" "http" "git" "ssh" "file"];

        # Functional tests read the API anonymously: a documented public
        # read-only posture. Secure read defaults are exercised explicitly.
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

      declarative = {
        api_keys = [
          {
            name = "bootstrap-admin";
            key_file = toString (pkgs.writeText "bootstrap-admin-key" "circus_bootstrap_key");
            role = "admin";
          }
        ];

        projects = [
          {
            name = "declarative-project";
            repository_url = "https://github.com/test/declarative";
            description = "Test declarative project";
            jobsets = [
              {
                name = "packages";
                nix_expression = "packages";
                flake_mode = true;
                enabled = true;
                check_interval = 3600;
                state = "disabled";
              }
            ];
          }
        ];
      };
    };
  };
}
