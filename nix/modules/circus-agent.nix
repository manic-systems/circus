self: {
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkIf;
  inherit (lib.attrsets) recursiveUpdate;
  settingsFormat = pkgs.formats.toml {};

  cfg = config.services.circus-agent;
  configFile = settingsFormat.generate "circus-agent.toml" (recursiveUpdate cfg.settings {
    agent.auth_token = "@CIRCUS_AGENT_AUTH_TOKEN@";
    tracing.show_timestamps = false;
  });

in {
  imports = [(import ./circus-agent-options.nix self)];

  config = mkIf cfg.enable {
    users.users.circus-agent = {
      isSystemUser = true;
      group = "circus-agent";
      home = cfg.settings.agent.work_dir;
      createHome = true;
    };
    users.groups.circus-agent = {};

    nix.settings.extra-trusted-users = ["circus-agent"];

    systemd.services.circus-agent = {
      description = "Circus distributed build agent";
      after = ["network-online.target" "nix-daemon.service"];
      wants = ["network-online.target"];
      wantedBy = ["multi-user.target"];

      path = [config.nix.package];

      environment.SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";

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
          install -m 0600 /dev/null "$RUNTIME_DIRECTORY/circus-agent.toml"
          ${pkgs.jq}/bin/jq -Rrs --arg tok "$token" \
            '($tok | @json) as $j | gsub("\"@CIRCUS_AGENT_AUTH_TOKEN@\""; $j)' \
            ${configFile} > "$RUNTIME_DIRECTORY/circus-agent.toml"
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
