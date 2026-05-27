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

  cfg = config.services.circus;

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

  settingsFile = settingsFormat.generate "circus.toml" finalSettings;

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
          The raw API key value (e.g. "circus_mykey123").
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
  options.services.circus = {
    enable = mkEnableOption "circus system";

    # TODO: could we use `mkPackageOption` here?
    # Also for the options below
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
      type = settingsType;
      default = {};
      description = ''
        circus configuration as a Nix attribute set. Will be converted to TOML
        and written to {file}`circus.toml`.
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
            key = "circus_admin_secret";
            role = "admin";
          }
          {
            name = "ci-bot";
            key = "circus_bot_key";
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
            passwordFile = "/run/secrets/circus-admin-password";
            role = "admin";
          };
          readonly = {
            email = "readonly@example.com";
            passwordFile = "/run/secrets/circus-readonly-password";
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
      enable = mkEnableOption "circus server (REST API)";
    };

    evaluator = {
      enable = mkEnableOption "circus evaluator (Git polling and nix evaluation)";
    };

    queueRunner = {
      enable = mkEnableOption "circus queue runner (build dispatch)";
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

    users.users.circus = {
      isSystemUser = true;
      group = "circus";
      home = "/var/lib/circus";
      createHome = true;
    };

    users.groups.circus = {};

    # NOTE: needed by nix-eval-jobs to access the Nix daemon.
    # This is completely undocumented but used by other projects in a similar
    # fashion to solve the same problem without clobbering `allowed-users`.
    nix.settings.extra-allowed-users = [ "circus" ];

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
      server.host = "127.0.0.1";
      server.port = 3000;

      gc = {
        gc_roots_dir = mkDefault "/nix/var/nix/gcroots/per-user/circus/circus-roots";
        enabled = mkDefault true;
        max_age_days = mkDefault 30;
        cleanup_interval = mkDefault 3600;
      };

      logs.log_dir = mkDefault "/var/lib/circus/logs";
      cache.enabled = mkDefault true;
      evaluator.restrict_eval = mkDefault true;
      evaluator.allow_ifd = mkDefault false;
      signing.enabled = mkDefault false;
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
            ExecStartPre = "${cfg.migratePackage}/bin/circus-migrate up ${finalSettings.database.url or "postgresql:///circus?host=/run/postgresql"}";
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

          environment = {
            CIRCUS_CONFIG_FILE = "${settingsFile}";
          };
        };

        circus-evaluator = mkIf cfg.evaluator.enable {
          description = "circus Evaluator";
          wantedBy = ["multi-user.target"];
          after = ["network.target" "circus-server.service"] ++ optional cfg.database.createLocally "postgresql.target";
          requires = ["circus-server.service"] ++ optional cfg.database.createLocally "postgresql.target";

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

          environment = {
            CIRCUS_CONFIG_FILE = "${settingsFile}";
            CIRCUS_EVALUATOR__WORK_DIR = "/var/lib/circus/evaluator";
            CIRCUS_EVALUATOR__RESTRICT_EVAL = "true";
          };
        };

        circus-queue-runner = mkIf cfg.queueRunner.enable {
          description = "circus Queue Runner";
          wantedBy = ["multi-user.target"];
          after = ["network.target" "circus-server.service"] ++ optional cfg.database.createLocally "postgresql.target";
          requires = ["circus-server.service"] ++ optional cfg.database.createLocally "postgresql.target";

          path = with pkgs; [
            nix
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

          environment = {
            CIRCUS_CONFIG_FILE = "${settingsFile}";
            CIRCUS_QUEUE_RUNNER__WORK_DIR = "/var/lib/circus/queue-runner";
          };
        };
      };
    };
  };
}
