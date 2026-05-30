{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkIf;
  inherit (lib.options) mkOption mkEnableOption;
  inherit (lib.types) listOf package str path ints;
  tomlFormat = pkgs.formats.toml {};

  cfg = config.services.circus-agent;
  configFile = tomlFormat.generate "circus-agent.toml" {
    agent = {
      inherit (cfg) name systems supported_features mandatory_features max_jobs speed_factor;
      runner_url = cfg.runnerUrl;
      work_dir = cfg.workDir;
      heartbeat_interval_secs = cfg.heartbeatInterval;
      reconnect_delay_secs = cfg.reconnectDelay;
    };
  };
in {
  options.services.circus-agent = {
    enable = mkEnableOption "Circus distributed build agent";

    package = mkOption {
      type = package;
      description = "circus-agent package to use.";
    };

    name = mkOption {
      type = str;
      example = "build-01";
      description = "Operator-assigned agent name; unique within the cluster.";
    };

    runnerUrl = mkOption {
      type = str;
      example = "circus://runner.internal:8443";
      description = ''
        Queue-runner endpoint. Accepts `circus://host:port` and
        `circus+tls://host:port`. The scheme picks the transport.
      '';
    };

    authTokenFile = mkOption {
      type = path;
      description = ''
        Path to a file containing the bearer token. Mode 0400 owned by
        the circus-agent user. The token is templated into the runtime
        config via systemd's LoadCredential mechanism.
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

    workDir = mkOption {
      type = path;
      default = "/var/lib/circus-agent";
    };

    heartbeatInterval = mkOption {
      type = ints.positive;
      default = 10;
    };

    reconnectDelay = mkOption {
      type = ints.positive;
      default = 5;
    };
  };

  config = mkIf cfg.enable {
    users.users.circus-agent = {
      isSystemUser = true;
      group = "circus-agent";
      home = cfg.workDir;
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
        WorkingDirectory = cfg.workDir;

        # Render the auth token into a runtime config that is private to
        # this unit. The token never lands in the Nix store.
        LoadCredential = "auth_token:${cfg.authTokenFile}";
        ExecStartPre = pkgs.writeShellScript "circus-agent-render-config" ''
          set -eu
          token="$(cat "$CREDENTIALS_DIRECTORY/auth_token")"
          install -m 0600 ${configFile} "$RUNTIME_DIRECTORY/circus-agent.toml"
          {
            printf 'auth_token = '
            printf '%s' "$token" | ${pkgs.jq}/bin/jq -Rs .
          } >> "$RUNTIME_DIRECTORY/circus-agent.toml"
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
