{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkIf mkDefault;
  inherit (lib.options) mkOption mkEnableOption literalExpression;
  inherit (lib.types) bool str int package listOf submodule nullOr enum attrsOf;
  inherit (lib.attrsets) recursiveUpdate optionalAttrs mapAttrsToList filterAttrs;
  inherit (lib.lists) optional map;

  cfg = config.services.fc-ci;

  settingsFormat = pkgs.formats.toml {};
  settingsType = settingsFormat.type;

  # Build the final settings by merging declarative config into settings
  finalSettings = recursiveUpdate cfg.settings (optionalAttrs (cfg.declarative.projects != [] || cfg.declarative.apiKeys != [] || cfg.declarative.users != {} || cfg.declarative.remoteBuilders != []) {
    declarative = {
      projects = map (p:
        filterAttrs (_: v: v != null) {
          name = p.name;
          repository_url = p.repositoryUrl;
          description = p.description;
          jobsets = map (j:
            filterAttrs (_: v: v != null) {
              name = j.name;
              nix_expression = j.nixExpression;
              enabled = j.enabled;
              flake_mode = j.flakeMode;
              check_interval = j.checkInterval;
              state = j.state;
              branch = j.branch;
              scheduling_shares = j.schedulingShares;
              keep_nr = j.keepNr;
              inputs = map (i:
                filterAttrs (_: v: v != null) {
                  name = i.name;
                  input_type = i.inputType;
                  value = i.value;
                  revision = i.revision;
                })
              j.inputs;
            })
          p.jobsets;
          notifications =
            map (n: {
              notification_type = n.notificationType;
              config = n.config;
              enabled = n.enabled;
            })
            p.notifications;
          webhooks = map (w:
            filterAttrs (_: v: v != null) {
              forge_type = w.forgeType;
              secret_file = w.secretFile;
              enabled = w.enabled;
            })
          p.webhooks;
          channels =
            map (c: {
              name = c.name;
              jobset_name = c.jobsetName;
            })
            p.channels;
          members =
            map (m: {
              username = m.username;
              role = m.role;
            })
            p.members;
        })
      cfg.declarative.projects;

      api_keys =
        map (k: {
          name = k.name;
          key = k.key;
          role = k.role;
        })
        cfg.declarative.apiKeys;

      users = mapAttrsToList (username: u: let
        hasInlinePassword = u.password != null;
        _ =
          if hasInlinePassword
          then builtins.throw "User '${username}' has inline password set. Use passwordFile instead to avoid plaintext passwords in the Nix store."
          else null;
      in
        filterAttrs (_: v: v != null) {
          inherit username;
          email = u.email;
          full_name = u.fullName;
          password_file = u.passwordFile;
          role = u.role;
          enabled = u.enabled;
        })
      cfg.declarative.users;

      remote_builders = map (b:
        filterAttrs (_: v: v != null) {
          name = b.name;
          ssh_uri = b.sshUri;
          systems = b.systems;
          max_jobs = b.maxJobs;
          speed_factor = b.speedFactor;
          supported_features = b.supportedFeatures;
          mandatory_features = b.mandatoryFeatures;
          ssh_key_file = b.sshKeyFile;
          public_host_key = b.publicHostKey;
          enabled = b.enabled;
        })
      cfg.declarative.remoteBuilders;
    };
  });

  settingsFile = settingsFormat.generate "fc.toml" finalSettings;

  jobsetOpts = {
    options = {
      enabled = mkOption {
        type = bool;
        default = true;
        description = "Whether this jobset is enabled for evaluation. Deprecated: use `state` instead.";
      };

      name = mkOption {
        type = str;
        description = "Jobset name.";
      };

      nixExpression = mkOption {
        type = str;
        default = "hydraJobs";
        example = literalExpression "packages // checks";
        description = "Nix expression to evaluate (e.g. 'packages', 'checks', 'hydraJobs').";
      };

      flakeMode = mkOption {
        type = bool;
        default = true;
        description = "Whether to evaluate as a flake.";
      };

      checkInterval = mkOption {
        type = int;
        default = 60;
        description = "Seconds between evaluation checks.";
      };

      state = mkOption {
        type = enum ["disabled" "enabled" "one_shot" "one_at_a_time"];
        default = "enabled";
        description = ''
          Jobset scheduling state:

          * `disabled`: Jobset will not be evaluated
          * `enabled`: Normal operation, evaluated according to checkInterval
          * `one_shot`: Evaluated once, then automatically set to disabled
          * `one_at_a_time`: Only one build can run at a time for this jobset
        '';
      };

      branch = mkOption {
        type = nullOr str;
        default = null;
        description = "Git branch to track. Defaults to repository default branch.";
      };

      schedulingShares = mkOption {
        type = int;
        default = 100;
        description = "Scheduling priority shares. Higher values = more priority.";
      };

      keepNr = mkOption {
        type = int;
        default = 3;
        description = "Number of recent successful evaluations to retain for GC pinning.";
      };

      inputs = mkOption {
        type = listOf (submodule {
          options = {
            name = mkOption {
              type = str;
              description = "Input name.";
            };
            inputType = mkOption {
              type = str;
              default = "git";
              description = "Input type: git, string, boolean, path, or build.";
            };
            value = mkOption {
              type = str;
              description = "Input value.";
            };
            revision = mkOption {
              type = nullOr str;
              default = null;
              description = "Git revision (for git inputs).";
            };
          };
        });
        default = [];
        description = "Jobset inputs for parameterized evaluations.";
      };
    };
  };

  projectOpts = {
    options = {
      name = mkOption {
        type = str;
        description = "Project name (unique identifier).";
      };

      repositoryUrl = mkOption {
        type = str;
        description = "Git repository URL.";
      };

      description = mkOption {
        type = nullOr str;
        default = null;
        description = "Optional project description.";
      };

      jobsets = mkOption {
        type = listOf (submodule jobsetOpts);
        default = [];
        description = "Jobsets to create for this project.";
      };

      notifications = mkOption {
        type = listOf (submodule {
          options = {
            notificationType = mkOption {
              type = str;
              description = "Notification type: github_status, email, gitlab_status, gitea_status, webhook.";
            };
            config = mkOption {
              type = settingsType;
              default = {};
              description = "Type-specific configuration.";
            };
            enabled = mkOption {
              type = bool;
              default = true;
              description = "Whether this notification is enabled.";
            };
          };
        });
        default = [];
        description = "Notification configurations for this project.";
      };

      webhooks = mkOption {
        type = listOf (submodule {
          options = {
            forgeType = mkOption {
              type = enum ["github" "gitea" "gitlab"];
              description = "Forge type for webhook.";
            };
            secretFile = mkOption {
              type = nullOr str;
              default = null;
              description = "Path to file containing webhook secret.";
            };
            enabled = mkOption {
              type = bool;
              default = true;
              description = "Whether this webhook is enabled.";
            };
          };
        });
        default = [];
        description = "Webhook configurations for this project.";
      };

      channels = mkOption {
        type = listOf (submodule {
          options = {
            name = mkOption {
              type = str;
              description = "Channel name.";
            };
            jobsetName = mkOption {
              type = str;
              description = "Name of the jobset this channel tracks.";
            };
          };
        });
        default = [];
        description = "Release channels for this project.";
      };

      members = mkOption {
        type = listOf (submodule {
          options = {
            username = mkOption {
              type = str;
              description = "Username of the member.";
            };
            role = mkOption {
              type = enum ["member" "maintainer" "admin"];
              default = "member";
              description = "Project role for the member.";
            };
          };
        });
        default = [];
        description = "Project members with their roles.";
      };
    };
  };

  apiKeyOpts = {
    options = {
      name = mkOption {
        type = str;
        description = "Human-readable name for this API key.";
      };

      key = mkOption {
        type = str;
        description = ''
          The raw API key value (e.g. "fc_mykey123").
          Will be hashed before storage. Consider using a secrets manager.
        '';
      };

      # FIXME: should be a list, ideally
      role = mkOption {
        type = str;
        default = "admin";
        example = "eval-jobset";
        description = ''
          Role, one of:

          * admin,
          * read-only,
          * create-projects,
          * eval-jobset,
          * cancel-build,
          * restart-jobs,
          * bump-to-front.
        '';
      };
    };
  };

  userOpts = {
    options = {
      enabled = mkOption {
        type = bool;
        default = true;
        description = "Whether this user is enabled.";
      };

      email = mkOption {
        type = str;
        description = "User's email address.";
      };

      fullName = mkOption {
        type = nullOr str;
        default = null;
        description = "Optional full name for the user.";
      };

      password = mkOption {
        type = nullOr str;
        default = null;
        description = ''
          Password provided inline (for dev/testing only).
          For production, use {option}`passwordFile` instead.
        '';
      };

      passwordFile = mkOption {
        type = nullOr str;
        default = null;
        description = ''
          Path to a file containing the user's password.
          Preferred for production deployments.
        '';
      };

      role = mkOption {
        type = str;
        default = "read-only";
        example = "eval-jobset";
        description = ''
          Role, one of:

          * admin,
          * read-only,
          * create-projects,
          * eval-jobset,
          * cancel-build,
          * restart-jobs,
          * bump-to-front.
        '';
      };
    };
  };

  remoteBuilderOpts = {
    options = {
      name = mkOption {
        type = str;
        description = "Unique name for this builder.";
      };

      sshUri = mkOption {
        type = str;
        example = "ssh://builder@builder.example.com";
        description = "SSH URI for connecting to the builder.";
      };

      systems = mkOption {
        type = listOf str;
        default = ["x86_64-linux"];
        description = "List of systems this builder supports.";
      };

      maxJobs = mkOption {
        type = int;
        default = 1;
        description = "Maximum number of parallel jobs.";
      };

      speedFactor = mkOption {
        type = int;
        default = 1;
        description = "Speed factor for scheduling (higher = faster builder).";
      };

      supportedFeatures = mkOption {
        type = listOf str;
        default = [];
        description = "List of supported features.";
      };

      mandatoryFeatures = mkOption {
        type = listOf str;
        default = [];
        description = "List of mandatory features.";
      };

      sshKeyFile = mkOption {
        type = nullOr str;
        default = null;
        description = "Path to SSH private key file.";
      };

      publicHostKey = mkOption {
        type = nullOr str;
        default = null;
        description = "SSH public host key for verification.";
      };

      enabled = mkOption {
        type = bool;
        default = true;
        description = "Whether this builder is enabled.";
      };
    };
  };
in {
  options.services.fc-ci = {
    enable = mkEnableOption "FC CI system";

    # TODO: could we use `mkPackageOption` here?
    # Also for the options below
    package = mkOption {
      type = package;
      description = "The FC server package.";
    };

    evaluatorPackage = mkOption {
      type = package;
      default = cfg.package;
      defaultText = "cfg.package";
      description = "The FC evaluator package.";
    };

    queueRunnerPackage = mkOption {
      type = package;
      default = cfg.package;
      defaultText = "cfg.package";
      description = "The FC queue runner package.";
    };

    migratePackage = mkOption {
      type = package;
      description = "The FC migration CLI package.";
    };

    settings = mkOption {
      type = settingsType;
      default = {};
      description = ''
        FC configuration as a Nix attribute set. Will be converted to TOML
        and written to {file}`fc.toml`.
      '';
    };

    declarative = {
      projects = mkOption {
        type = listOf (submodule projectOpts);
        default = [];
        description = ''
          Declarative project definitions. These are upserted on every
          server startup, ensuring the database matches this configuration.
        '';
        example = [
          {
            name = "my-project";
            repositoryUrl = "https://github.com/user/repo";
            description = "My Nix project";
            jobsets = [
              {
                name = "packages";
                nixExpression = "packages";
              }
              {
                name = "checks";
                nixExpression = "checks";
              }
            ];
          }
        ];
      };

      apiKeys = mkOption {
        type = listOf (submodule apiKeyOpts);
        default = [];
        description = ''
          Declarative API key definitions. Keys are upserted on every
          server startup. Use a secrets manager for production deployments.
        '';
        example = [
          {
            name = "admin";
            key = "fc_admin_secret";
            role = "admin";
          }
          {
            name = "ci-bot";
            key = "fc_ci_bot_key";
            role = "eval-jobset";
          }
        ];
      };

      users = mkOption {
        type = attrsOf (submodule userOpts);
        default = {};
        description = ''
          Declarative user definitions. The attribute name is the username.
          Users are upserted on every server startup.

          Use {option}`passwordFile` with a secrets manager for production deployments.
        '';
        example = {
          admin = {
            email = "admin@example.com";
            passwordFile = "/run/secrets/fc-admin-password";
            role = "admin";
          };
          readonly = {
            email = "readonly@example.com";
            passwordFile = "/run/secrets/fc-readonly-password";
            role = "read-only";
          };
        };
      };

      remoteBuilders = mkOption {
        type = listOf (submodule remoteBuilderOpts);
        default = [];
        description = ''
          Declarative remote builder definitions. Builders are upserted on every
          server startup for distributed builds.
        '';
        example = [
          {
            name = "builder1";
            sshUri = "ssh://builder@builder.example.com";
            systems = ["x86_64-linux" "aarch64-linux"];
            maxJobs = 4;
            speedFactor = 2;
          }
        ];
      };
    };

    database = {
      createLocally = mkOption {
        type = bool;
        default = true;
        description = "Whether to create the PostgreSQL database locally.";
      };
    };

    server = {
      enable = mkEnableOption "FC server (REST API)";
    };

    evaluator = {
      enable = mkEnableOption "FC evaluator (Git polling and nix evaluation)";
    };

    queueRunner = {
      enable = mkEnableOption "FC queue runner (build dispatch)";
    };
  };

  config = mkIf cfg.enable {
    assertions =
      mapAttrsToList (
        username: user: {
          assertion = user.password != null || user.passwordFile != null;
          message = "User '${username}' must have either 'password' or 'passwordFile' set.";
        }
      )
      cfg.declarative.users;

    users.users.fc = {
      isSystemUser = true;
      group = "fc";
      home = "/var/lib/fc";
      createHome = true;
    };

    users.groups.fc = {};

    services.postgresql = mkIf cfg.database.createLocally {
      enable = true;
      ensureDatabases = ["fc"];
      ensureUsers = [
        {
          name = "fc";
          ensureDBOwnership = true;
        }
      ];
    };

    services.fc-ci.settings = {
      database.url = mkDefault "postgresql:///fc?host=/run/postgresql";
      server.host = mkDefault "127.0.0.1";
      server.port = mkDefault 3000;

      gc = {
        gc_roots_dir = mkDefault "/nix/var/nix/gcroots/per-user/fc/fc-roots";
        enabled = mkDefault true;
        max_age_days = mkDefault 30;
        cleanup_interval = mkDefault 3600;
      };

      logs.log_dir = mkDefault "/var/lib/fc/logs";
      cache.enabled = mkDefault true;
      evaluator.restrict_eval = mkDefault true;
      evaluator.allow_ifd = mkDefault false;
      signing.enabled = mkDefault false;
    };

    systemd = {
      tmpfiles.rules = [
        (mkIf cfg.server.enable "d /var/lib/fc/logs 0750 fc fc -")
        (mkIf cfg.queueRunner.enable "d /nix/var/nix/gcroots/per-user/fc 0755 fc fc -")
      ];

      services = {
        fc-server = mkIf cfg.server.enable {
          description = "FC CI Server";
          wantedBy = ["multi-user.target"];
          after = ["network.target"] ++ optional cfg.database.createLocally "postgresql.target";
          requires = optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [nix zstd];

          serviceConfig = {
            ExecStartPre = "${cfg.migratePackage}/bin/fc-migrate up ${finalSettings.database.url or "postgresql:///fc?host=/run/postgresql"}";
            ExecStart = "${cfg.package}/bin/fc-server";
            Restart = "on-failure";
            RestartSec = 5;
            User = "fc";
            Group = "fc";
            StateDirectory = "fc";
            LogsDirectory = "fc";
            WorkingDirectory = "/var/lib/fc";
            ReadWritePaths = ["/var/lib/fc"];

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

          environment = {
            FC_CONFIG_FILE = "${settingsFile}";
          };
        };

        fc-evaluator = mkIf cfg.evaluator.enable {
          description = "FC CI Evaluator";
          wantedBy = ["multi-user.target"];
          after = ["network.target" "fc-server.service"] ++ optional cfg.database.createLocally "postgresql.target";
          requires = ["fc-server.service"] ++ optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [
            nix
            git
            nix-eval-jobs
          ];

          serviceConfig = {
            ExecStart = "${cfg.evaluatorPackage}/bin/fc-evaluator";
            Restart = "on-failure";
            RestartSec = 10;
            User = "fc";
            Group = "fc";
            StateDirectory = "fc";
            WorkingDirectory = "/var/lib/fc";
            ReadWritePaths = ["/var/lib/fc"];

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

          environment = {
            FC_CONFIG_FILE = "${settingsFile}";
            FC_EVALUATOR__WORK_DIR = "/var/lib/fc/evaluator";
            FC_EVALUATOR__RESTRICT_EVAL = "true";
          };
        };

        fc-queue-runner = mkIf cfg.queueRunner.enable {
          description = "FC CI Queue Runner";
          wantedBy = ["multi-user.target"];
          after = ["network.target" "fc-server.service"] ++ optional cfg.database.createLocally "postgresql.target";
          requires = ["fc-server.service"] ++ optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [
            nix
          ];

          serviceConfig = {
            ExecStart = "${cfg.queueRunnerPackage}/bin/fc-queue-runner";
            Restart = "on-failure";
            RestartSec = 10;
            User = "fc";
            Group = "fc";
            StateDirectory = "fc";
            LogsDirectory = "fc";
            WorkingDirectory = "/var/lib/fc";
            ReadWritePaths = [
              "/var/lib/fc"
              "/nix/var/nix/gcroots/per-user/fc"
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

          environment = {
            FC_CONFIG_FILE = "${settingsFile}";
            FC_QUEUE_RUNNER__WORK_DIR = "/var/lib/fc/queue-runner";
          };
        };
      };
    };
  };
}
