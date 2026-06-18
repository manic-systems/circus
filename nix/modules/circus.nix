{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkDefault mkIf;
  inherit (lib.options) literalExpression mkEnableOption mkOption;
  inherit (lib.types) attrsOf bool int listOf nullOr package path port str submodule;
  inherit (lib.lists) optional;

  cfg = config.services.circus;

  settingsFormat = pkgs.formats.toml {};
  settingsType = settingsFormat.type;

  jobsetSettings = submodule {
    freeformType = settingsType;
    options = {
      name = mkOption {type = str;};
      nix_expression = mkOption {type = str;};
    };
  };

  projectSettings = submodule {
    freeformType = settingsType;
    options = {
      name = mkOption {type = str;};
      repository_url = mkOption {type = str;};
      jobsets = mkOption {
        type = listOf jobsetSettings;
        default = [];
      };
    };
  };

  apiKeySettings = submodule {
    freeformType = settingsType;
    options.name = mkOption {type = str;};
  };

  userSettings = submodule {
    freeformType = settingsType;
    options = {
      username = mkOption {type = str;};
      email = mkOption {type = str;};
    };
  };

  remoteBuilderSettings = submodule {
    freeformType = settingsType;
    options = {
      name = mkOption {type = str;};
      ssh_uri = mkOption {type = str;};
    };
  };

  settingsSubmodule = submodule {
    freeformType = settingsType;
    options = {
      database = mkOption {
        type = submodule {
          freeformType = settingsType;
          options.url = mkOption {
            type = str;
            description = "PostgreSQL connection URL used by all Circus services.";
          };
        };
        default = {};
        description = "Database settings.";
      };

      server = mkOption {
        type = settingsType;
        default = {};
        description = "HTTP server settings.";
      };

      ui = mkOption {
        type = settingsType;
        default = {};
        description = "Dashboard UI settings.";
      };

      declarative = mkOption {
        type = submodule {
          freeformType = settingsType;
          options = {
            projects = mkOption {
              type = listOf projectSettings;
            };
            api_keys = mkOption {
              type = listOf apiKeySettings;
            };
            users = mkOption {
              type = listOf userSettings;
            };
            remote_builders = mkOption {
              type = listOf remoteBuilderSettings;
            };
          };
        };
        default = {};
        description = "Declarative bootstrap settings.";
      };
    };
  };

  settingsFile = settingsFormat.generate "circus.toml" cfg.settings;
in {
  options.services.circus = {
    enable = mkEnableOption "circus system";

    package = mkOption {
      type = package;
      description = "The circus server package.";
    };

    evaluatorPackage = mkOption {
      type = package;
      default = cfg.package;
      defaultText = "cfg.package";
      description = "The circus evaluator package.";
    };

    queueRunnerPackage = mkOption {
      type = package;
      default = cfg.package;
      defaultText = "cfg.package";
      description = "The circus queue runner package.";
    };

    migratePackage = mkOption {
      type = package;
      description = "The circus migration CLI package.";
    };

    settings = mkOption {
      type = settingsSubmodule;
      default = {};
      description = ''
        Circus configuration as a Nix attribute set. This is converted directly
        to TOML and written to {file}`circus.toml`; option names match the TOML
        schema.
      '';
      example = literalExpression ''
        {
          server.port = 3000;
          declarative.projects = [
            {
              name = "my-project";
              repository_url = "https://github.com/user/repo";
              jobsets = [{ name = "packages"; nix_expression = "packages"; }];
            }
          ];
        }
      '';
    };

    database.createLocally = mkOption {
      type = bool;
      default = true;
      description = "Whether to create the PostgreSQL database locally.";
    };

    server.enable = mkEnableOption "circus server (REST API)";
    evaluator.enable = mkEnableOption "circus evaluator (Git polling and nix evaluation)";
    queueRunner.enable = mkEnableOption "circus queue runner (build dispatch)";
  };

  config = mkIf cfg.enable {
    users.users.circus = {
      isSystemUser = true;
      group = "circus";
      home = "/var/lib/circus";
      createHome = true;
    };

    users.groups.circus = {};
    nix.settings = {
      # NOTE: needed by nix-eval-jobs to access the Nix daemon.
      # This is completely undocumented but used by other projects in a similar
      # fashion to solve the same problem without clobbering `allowed-users`.
      extra-allowed-users = ["circus"];

      # The queue runner builds with `--option sandbox true` and
      # `--max-build-log-size`; these are restricted settings that the daemon
      # ignores unless the requesting user is trusted. It also runs
      # `nix-store --import` to pull agent-built closures into the runner
      # store. Trust circus for both.
      extra-trusted-users = ["circus"];
    };

    services.postgresql = mkIf cfg.database.createLocally {
      enable = true;
      ensureDatabases = ["circus"];
      ensureUsers = [
        {
          name = "circus";
          ensureDBOwnership = true;
        }
      ];
    };

    services.circus.settings = mkDefault {
      database.url = "postgresql:///circus?host=/run/postgresql";
    };

    systemd = {
      tmpfiles.rules = [
        (mkIf cfg.server.enable "d /var/lib/circus/logs 0750 circus circus -")
        (mkIf cfg.queueRunner.enable "d /nix/var/nix/gcroots/per-user/circus 0755 circus circus -")
      ];

      services = {
        circus-server = mkIf cfg.server.enable {
          description = "circus Server";
          wantedBy = ["multi-user.target"];
          after = ["network.target"] ++ optional cfg.database.createLocally "postgresql.target";
          requires = optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [nix zstd];

          serviceConfig = {
            ExecStartPre = "${cfg.migratePackage}/bin/circusctl migrate up ${cfg.settings.database.url}";
            ExecStart = "${cfg.package}/bin/circus-server";
            Restart = "on-failure";
            RestartSec = 5;
            User = "circus";
            Group = "circus";
            StateDirectory = "circus";
            LogsDirectory = "circus";
            WorkingDirectory = "/var/lib/circus";
            ReadWritePaths = ["/var/lib/circus"];

            # Hardening
            ProtectSystem = "strict";
            ProtectHome = true;
            NoNewPrivileges = true;
            PrivateTmp = true;
            ProtectKernelTunables = true;
            ProtectKernelModules = true;
            ProtectControlGroups = true;
            RestrictSUIDSGID = true;
          };

          environment.CIRCUS_CONFIG_FILE = "${settingsFile}";
        };

        circus-evaluator = mkIf cfg.evaluator.enable {
          description = "circus Evaluator";
          wantedBy = ["multi-user.target"];
          after = ["network.target"] ++ optional cfg.server.enable "circus-server.service" ++ optional cfg.database.createLocally "postgresql.target";
          requires = optional cfg.server.enable "circus-server.service" ++ optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [
            nix
            git
            nix-eval-jobs
          ];

          serviceConfig = {
            ExecStart = "${cfg.evaluatorPackage}/bin/circus-evaluator";
            Restart = "on-failure";
            RestartSec = 10;
            User = "circus";
            Group = "circus";
            StateDirectory = "circus";
            WorkingDirectory = "/var/lib/circus";
            ReadWritePaths = ["/var/lib/circus"];

            # Hardening
            ProtectSystem = "strict";
            ProtectHome = true;
            NoNewPrivileges = true;
            PrivateTmp = true;
            ProtectKernelTunables = true;
            ProtectKernelModules = true;
            ProtectControlGroups = true;
            RestrictSUIDSGID = true;
          };

          environment.CIRCUS_CONFIG_FILE = "${settingsFile}";
        };

        circus-queue-runner = mkIf cfg.queueRunner.enable {
          description = "circus Queue Runner";
          wantedBy = ["multi-user.target"];
          after = ["network.target"] ++ optional cfg.server.enable "circus-server.service" ++ optional cfg.database.createLocally "postgresql.target";
          requires = optional cfg.server.enable "circus-server.service" ++ optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [
            nix
            openssh
          ];

          serviceConfig = {
            ExecStart = "${cfg.queueRunnerPackage}/bin/circus-queue-runner";
            Restart = "on-failure";
            RestartSec = 10;
            User = "circus";
            Group = "circus";
            StateDirectory = "circus";
            LogsDirectory = "circus";
            WorkingDirectory = "/var/lib/circus";
            ReadWritePaths = [
              "/var/lib/circus"
              "/nix/var/nix/gcroots/per-user/circus"
            ];

            # Hardening
            ProtectSystem = "strict";
            ProtectHome = true;
            NoNewPrivileges = true;
            PrivateTmp = true;
            ProtectKernelTunables = true;
            ProtectKernelModules = true;
            ProtectControlGroups = true;
            RestrictSUIDSGID = true;
          };

          environment.CIRCUS_CONFIG_FILE = "${settingsFile}";
        };
      };
    };
  };
}
