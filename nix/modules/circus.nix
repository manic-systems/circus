{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkIf mkDefault mkMerge;
  inherit (lib.options) mkEnableOption mkOption;
  inherit (lib.types) bool int listOf nullOr package path str submodule;
  inherit (lib.lists) concatMap optional;
  inherit (lib.meta) getExe';

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

  cacheUpstreamSettings = submodule {
    freeformType = settingsType;
    options = {
      url = mkOption {
        type = str;
        description = "Upstream binary cache URL (substituter).";
        example = "https://cache.nixos.org";
      };

      public_key = mkOption {
        type = nullOr str;
        default = null;
        example = "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=";
        description = "Trusted public key for the upstream cache.";
      };
    };
  };

  projectSettings = submodule {
    freeformType = settingsType;
    options = {
      name = mkOption {type = str;};
      repository_url = mkOption {type = str;};
      cache_enabled = mkOption {
        type = bool;
        default = false;
        description = ''
          Whether this project serves a per-project binary cache at
          `/projects/<name>/nix-cache/`.
        '';
      };

      cache_url = mkOption {
        type = nullOr str;
        default = null;
        example = "https://ci.example.org/projects/myproject/nix-cache/";
        description = ''
          Public substituter URL advertised for this project's cache. When `null`,
          consumers use `<site>/projects/<name>/nix-cache/` derived from the
          global `cache.cache_url`.
        '';
      };

      cache_upstreams = mkOption {
        type = listOf cacheUpstreamSettings;
        default = [];
        description = "Upstream caches this project's cache may fall through to.";
      };

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
            allow_runtime_mutation = mkOption {
              type = types.bool;
              default = false;
              description = "Allow runtime mutation of declaratively managed projects.";
            };
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

      cache = mkOption {
        type = submodule {
          freeformType = settingsType;
          options = {
            enabled = mkOption {
              type = bool;
              default = true;
              description = "Serve the global binary cache at `/nix-cache/`.";
            };
            secret_key_file = mkOption {
              type = nullOr path;
              default = null;
              description = ''
                Path to the Nix cache signing secret key.

                Generate with `nix key generate-secret`; distribute the matching
                public key to consumers as a `trusted-public-keys` entry.
              '';
            };
            cache_url = mkOption {
              type = nullOr str;
              default = null;
              example = "https://ci.example.org/nix-cache";
              description = "Public substituter URL of the global cache.";
            };

            upstreams = mkOption {
              type = listOf cacheUpstreamSettings;
              default = [];
              description = "Upstream caches the global cache may fall through to.";
            };
          };
        };
        default = {};
        description = "Global binary cache settings.";
      };

      signing = mkOption {
        type = submodule {
          freeformType = settingsType;
          options = {
            enabled = mkOption {
              type = bool;
              default = false;
              description = "Sign built store paths so substituters can trust them.";
            };
            key_file = mkOption {
              type = nullOr path;
              default = null;
              description = ''
                Path to the Nix signing secret key file (`<name>:<base64>` form).
                The server derives the public key from it for the dashboard's
                "How to use" panel.
              '';
            };
          };
        };
        default = {};
        description = "Build output signing settings.";
      };

      # Mirrors `circus_config::CacheUploadConfig`.
      cache_upload = mkOption {
        type = submodule {
          freeformType = settingsType;
          options = {
            enabled = mkOption {
              type = bool;
              default = false;
              description = "Push built paths to a remote store (e.g., S3).";
            };
            store_uri = mkOption {
              type = nullOr str;
              default = null;
              description = "Target store URI for uploads.";
              example = "s3://my-bucket?region=us-east-1";
            };
            upload_concurrency = mkOption {
              type = int;
              default = 4;
              description = "Concurrent upload workers.";
            };
            upload_max_retries = mkOption {
              type = int;
              default = 3;
              description = "Max retry attempts per path before giving up.";
            };
            fail_build_on_upload_error = mkOption {
              type = bool;
              default = false;
              description = "Fail the build when cache upload exhausts its retries.";
            };
            compression = mkOption {
              type = str;
              default = "zstd";
              description = "Wire compression for uploads: zstd, xz, gzip, or none.";
            };
            s3 = mkOption {
              type = nullOr (submodule {
                freeformType = settingsType;
                options = {
                  region = mkOption {
                    type = nullOr str;
                    default = null;
                    description = "AWS region (e.g. us-east-1).";
                  };
                  prefix = mkOption {
                    type = nullOr str;
                    default = null;
                    description = "Path prefix within the bucket.";
                  };
                  access_key_id = mkOption {
                    type = nullOr str;
                    default = null;
                    description = "AWS access key ID for presigned uploads.";
                  };
                  secret_access_key_file = mkOption {
                    type = nullOr path;
                    default = null;
                    description = "Path to a file holding the AWS secret access key.";
                  };
                };
              });
              default = null;
              description = "S3-specific upload settings (used when store_uri is s3://).";
            };
          };
        };
        default = {};
        description = "Build output cache upload settings.";
      };
    };
  };

  # Typed options default optional fields to null so they are discoverable, but
  # TOML has no null: drop null leaves (at any depth, including inside list
  # elements) before generating. An absent key is read back as `None` by serde,
  # which is the intended meaning.
  # FIXME: this sucks
  stripNulls = value:
    if lib.isAttrs value && !(lib.isDerivation value)
    then lib.mapAttrs (_: stripNulls) (lib.filterAttrs (_: v: v != null) value)
    else if lib.isList value
    then map stripNulls value
    else value;

  settingsFile = settingsFormat.generate "circus.toml" (stripNulls cfg.settings);
in {
  options.services.circus = {
    enable = mkEnableOption "circus system";

    package = mkOption {
      type = package;
      description = "The circus server package.";
    };

    evaluatorPackage = mkOption {
      type = package;
      description = "The circus evaluator package.";
    };

    queueRunnerPackage = mkOption {
      type = package;
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
      example = {
        server.port = 3000;
        declarative.projects = [
          {
            name = "my-project";
            repository_url = "https://github.com/user/repo";
            jobsets = [
              {
                name = "packages";
                nix_expression = "packages";
              }
            ];
          }
        ];
      };
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

    warnings = let
      declarative = cfg.settings.declarative or {};

      userWarnings = concatMap (
        user:
          optional (user ? password && user.password != null)
          ("services.circus.settings.declarative.users.\"${user.username}\": "
            + "'password' is set inline - this places the secret in the world-readable Nix store. "
            + "Use 'password_file' with a path to a file containing the password instead.")
      ) (declarative.users or []);

      apiKeyWarnings = concatMap (
        ak:
          optional (ak ? key && ak.key != null)
          ("services.circus.settings.declarative.api_keys.\"${ak.name}\": "
            + "'key' is set inline - this places the secret in the world-readable Nix store. "
            + "Use 'key_file' with a path to a file containing the API key instead.")
      ) (declarative.api_keys or []);

      webhookWarnings = concatMap (
        project:
          concatMap (
            webhook:
              optional (webhook ? secret && webhook.secret != null)
              ("services.circus.settings.declarative.projects.\"${project.name}\".webhooks: "
                + "'secret' is set inline - this places the secret in the world-readable Nix store. "
                + "Use 'secret_file' with a path to a file containing the webhook secret instead.")
          ) (project.webhooks or [])
      ) (declarative.projects or []);
    in
      userWarnings ++ apiKeyWarnings ++ webhookWarnings;

    nix.settings = {
      # NOTE: needed by the evaluator (evix) to access the Nix daemon.
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
      tmpfiles.settings."10-circus" = mkMerge [
        (mkIf cfg.server.enable {
          "/var/lib/circus/logs".d = {
            user = "circus";
            group = "circus";
            mode = "0750";
          };
        })

        (mkIf cfg.queueRunner.enable {
          "/nix/var/nix/gcroots/per-user/circus".d = {
            user = "circus";
            group = "circus";
            mode = "0755";
          };
        })
      ];

      services = {
        circus-server = mkIf cfg.server.enable {
          description = "circus Server";
          wantedBy = ["multi-user.target"];
          after = ["network.target"] ++ optional cfg.database.createLocally "postgresql.target";
          requires = optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [nix zstd];

          serviceConfig = {
            ExecStartPre = "${getExe' cfg.migratePackage "circusctl"} migrate up ${cfg.settings.database.url}";
            ExecStart = getExe' cfg.package "circus-server";
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
          ];

          serviceConfig = {
            ExecStart = getExe' cfg.evaluatorPackage "circus-evaluator";
            Restart = "on-failure";
            RestartSec = 10;
            User = "circus";
            Group = "circus";
            StateDirectory = "circus";
            WorkingDirectory = "/var/lib/circus";
            ReadWritePaths = ["/var/lib/circus"];
            # A single Nix evaluator can keep more than systemd's default
            # soft limit of 1024 files open while traversing a large flake.
            LimitNOFILE = 65536;

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
            ExecStart = getExe' cfg.queueRunnerPackage "circus-queue-runner";
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
