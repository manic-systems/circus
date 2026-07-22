{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkIf;
  inherit (lib.attrsets) optionalAttrs recursiveUpdate;
  settingsFormat = pkgs.formats.toml {};

  cfg = config.services.circus-agent;
  effectsEnabled =
    cfg.settings.agent ? effects || cfg.effectsSecretsFile != null;
  effectsSecretsPathComponents =
    if cfg.effectsSecretsFile == null
    then []
    else lib.drop 1 (lib.splitString "/" cfg.effectsSecretsFile);
  generatedSettings =
    recursiveUpdate
    {
      agent.auth_token = "@CIRCUS_AGENT_AUTH_TOKEN@";
      tracing.show_timestamps = false;
    }
    (optionalAttrs (cfg.effectsSecretsFile != null) {
      agent.effects.secrets_file = "@CIRCUS_AGENT_EFFECTS_SECRETS@";
    });
  configFile = settingsFormat.generate "circus-agent.toml" (recursiveUpdate cfg.settings generatedSettings);
in {
  imports = [./circus-agent-options.nix];

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion =
          cfg.effectsSecretsFile
          == null
          || (
            lib.hasPrefix "/" cfg.effectsSecretsFile
            && effectsSecretsPathComponents != []
            && lib.all (
              component:
                component
                != ""
                && component != "."
                && component != ".."
            )
            effectsSecretsPathComponents
            && cfg.effectsSecretsFile != builtins.storeDir
            && !lib.hasPrefix "${builtins.storeDir}/" cfg.effectsSecretsFile
          );
        message = ''
          services.circus-agent.effectsSecretsFile must be a canonical absolute
          runtime path outside the Nix store
        '';
      }
    ];

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

      serviceConfig =
        {
          Type = "simple";
          User = "circus-agent";
          Group = "circus-agent";
          StateDirectory = "circus-agent";
          StateDirectoryMode = "0750";
          WorkingDirectory = cfg.settings.agent.work_dir;

          # Render the auth token into a runtime config that is private to
          # this unit. The token never lands in the Nix store.
          LoadCredential =
            ["auth_token:${cfg.authTokenFile}"]
            ++ lib.optional (cfg.effectsSecretsFile != null)
            "effects_secrets:${cfg.effectsSecretsFile}";
          ExecStartPre = pkgs.writeShellScript "circus-agent-render-config" ''
            set -eu
            token="$(cat "$CREDENTIALS_DIRECTORY/auth_token")"
            effects_secrets=""
            ${lib.optionalString (cfg.effectsSecretsFile != null) ''
              effects_secrets="$RUNTIME_DIRECTORY/effects-secrets.json"
              install -m 0600 "$CREDENTIALS_DIRECTORY/effects_secrets" "$effects_secrets"
            ''}
            install -m 0600 /dev/null "$RUNTIME_DIRECTORY/circus-agent.toml"
            ${pkgs.jq}/bin/jq -Rrs \
              --arg token "$token" \
              --arg effects_secrets "$effects_secrets" \
              '($token | @json) as $token_json
               | ($effects_secrets | @json) as $effects_json
               | gsub("\"@CIRCUS_AGENT_AUTH_TOKEN@\""; $token_json)
               | gsub("\"@CIRCUS_AGENT_EFFECTS_SECRETS@\""; $effects_json)' \
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
          RestrictNamespaces =
            !(
              (cfg.settings.agent.rootless or false)
              || effectsEnabled
            );
          LockPersonality = true;
          MemoryDenyWriteExecute = true;
          SystemCallArchitectures = "native";
        }
        // optionalAttrs effectsEnabled {
          # ProtectKernelTunables masks files below /proc. On kernels before
          # Linux 7.2 those masks prevent an unprivileged user namespace from
          # mounting the fresh procfs required by the Effect PID namespace.
          # Starting process-only avoids the masks without exposing host kernel
          # tunables to either the agent or its Effects.
          ProcSubset = "pid";
        };
    };
  };
}
