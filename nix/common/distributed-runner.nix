{self}: {
  pkgs,
  lib,
  ...
}: let
  circus-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
in {
  imports = [self.nixosModules.circus];

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
    migratePackage = circus-packages.circus-cli;

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
          allow_plaintext = true;
          max_connections = 64;
          heartbeat_ttl_secs = 30;
          auth_tokens = [(builtins.hashString "sha256" "demo-agent-token-please-rotate")];
          cache_substituter = "http://runner:3000/nix-cache";
          cache_public_key = "circus-test-cache-1:umbC3dkmiWcreOM+p1+UWp0pJK4zOMcGqalT53h2PNw=";
        };
      };
    };

    settings.declarative.api_keys = [
      {
        name = "bootstrap-admin";
        key = "circus_bootstrap_key";
        role = "admin";
      }
    ];
  };
}
