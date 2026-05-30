{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkIf;
  inherit (lib.options) mkOption mkEnableOption;
  inherit (lib.types) listOf package str path ints submodule;
  inherit (lib.attrsets) recursiveUpdate;
  settingsFormat = pkgs.formats.toml {};

  cfg = config.services.circus-agent;
  configFile = settingsFormat.generate "circus-agent.toml" (recursiveUpdate cfg.settings {
    agent.auth_token = "@CIRCUS_AGENT_AUTH_TOKEN@";
  });
in {
  options.services.circus-agent = {
    enable = mkEnableOption "Circus distributed build agent";

    package = mkOption {
      type = package;
      description = "circus-agent package to use.";
    };

    authTokenFile = mkOption {
      type = path;
      description = ''
        Path to a file containing the bearer token. Mode 0400 owned by
        the circus-agent user. The token is rendered into the runtime
        config via systemd's LoadCredential mechanism.
      '';
    };

    settings = mkOption {
      type = submodule {
        freeformType = settingsFormat.type;
        options.agent = mkOption {
          type = submodule {
            freeformType = settingsFormat.type;
            options = {
              name = mkOption {
                type = str;
                example = "build-01";
                description = "Operator-assigned agent name; unique within the cluster.";
              };

              runner_url = mkOption {
                type = str;
                example = "circus://runner.internal:8443";
                description = ''
                  Queue-runner endpoint. Accepts `circus://host:port` and
                  `circus+tls://host:port`. The scheme picks the transport.
                '';
              };

              systems = mkOption {
                type = listOf str;
                default = [pkgs.stdenv.hostPlatform.system];
                description = "Nix systems this agent advertises.";
              };

              supported_features = mkOption {
                type = listOf str;
                default = [];
                description = "Optional Nix features the agent advertises (kvm, nixos-test, ...).";
              };

              mandatory_features = mkOption {
                type = listOf str;
                default = [];
                description = "Features the agent insists on; builds without them are skipped here.";
              };

              max_jobs = mkOption {
                type = ints.positive;
                default = 4;
              };

              speed_factor = mkOption {
                type = lib.types.numbers.positive;
                default = 1.0;
              };

              work_dir = mkOption {
                type = path;
                default = "/var/lib/circus-agent";
              };

              heartbeat_interval_secs = mkOption {
                type = ints.positive;
                default = 10;
              };

              reconnect_delay_secs = mkOption {
                type = ints.positive;
                default = 5;
              };
            };
          };
          default = {};
          description = "Settings for the `[agent]` section of `circus-agent.toml`.";
        };
      };
      default = {};
      description = ''
        `circus-agent.toml` as a Nix attribute set. The bearer token is
        intentionally not represented here; use `authTokenFile`.
      '';
    };
  };

  config = mkIf cfg.enable {
    users.users.circus-agent = {
      isSystemUser = true;
      group = "circus-agent";
      home = cfg.settings.agent.work_dir;
      createHome = true;
    };
    users.groups.circus-agent = {};

    systemd.services.circus-agent = {
      description = "Circus distributed build agent";
      after = ["network-online.target" "nix-daemon.service"];
      wants = ["network-online.target"];
      wantedBy = ["multi-user.target"];

      serviceConfig = {
        Type = "simple";
        User = "circus-agent";
        Group = "circus-agent";
        StateDirectory = "circus-agent";
        StateDirectoryMode = "0750";
        WorkingDirectory = cfg.settings.agent.work_dir;

        # Render the auth token into a runtime config that is private to
        # this unit. The token never lands in the Nix store.
        LoadCredential = "auth_token:${cfg.authTokenFile}";
        ExecStartPre = pkgs.writeShellScript "circus-agent-render-config" ''
          set -eu
          token="$(cat "$CREDENTIALS_DIRECTORY/auth_token")"
          token_json="$(printf '%s' "$token" | ${pkgs.jq}/bin/jq -Rs .)"
          install -m 0600 ${configFile} "$RUNTIME_DIRECTORY/circus-agent.toml"
          TOKEN_JSON="$token_json" ${pkgs.perl}/bin/perl -0pi -e \
            's/"\@CIRCUS_AGENT_AUTH_TOKEN\@"/$ENV{TOKEN_JSON}/g' \
            "$RUNTIME_DIRECTORY/circus-agent.toml"
        '';
        RuntimeDirectory = "circus-agent";
        RuntimeDirectoryMode = "0700";
        ExecStart = "${cfg.package}/bin/circus-agent --config %t/circus-agent/circus-agent.toml";
        Restart = "on-failure";
        RestartSec = "5s";

        # Hardening. Build agents do touch the Nix daemon socket and the
        # filesystem under StateDirectory; we keep everything else off.
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictNamespaces = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        SystemCallArchitectures = "native";
      };
    };
  };
}
