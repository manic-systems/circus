{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkIf;
  inherit (lib.options) mkOption;
  inherit (lib.types) ints str;
  inherit (lib.attrsets) recursiveUpdate;
  settingsFormat = pkgs.formats.toml {};

  cfg = config.services.circus-agent;

  nixBin =
    if config.nix.enable
    then "${config.nix.package}/bin"
    else "/nix/var/nix/profiles/default/bin";
  configTemplate = settingsFormat.generate "circus-agent.toml" (recursiveUpdate cfg.settings {
    agent.auth_token = "@CIRCUS_AGENT_AUTH_TOKEN@";
    tracing.show_timestamps = false;
  });
  runtimeConfig = "${toString cfg.settings.agent.work_dir}/circus-agent.runtime.toml";

  startScript = pkgs.writeShellScript "circus-agent-start" ''
    set -eu
    umask 077
    install -d -m 0750 ${lib.escapeShellArg (toString cfg.settings.agent.work_dir)}
    ${pkgs.jq}/bin/jq -Rrs --rawfile tok ${lib.escapeShellArg (toString cfg.authTokenFile)} \
      '($tok | sub("\\s+$"; "") | @json) as $j | gsub("\"@CIRCUS_AGENT_AUTH_TOKEN@\""; $j)' \
      ${configTemplate} > ${lib.escapeShellArg runtimeConfig}
    chmod 0600 ${lib.escapeShellArg runtimeConfig}
    exec ${cfg.package}/bin/circus-agent --config ${lib.escapeShellArg runtimeConfig}
  '';
in {
  imports = [./circus-agent-options.nix];

  options.services.circus-agent = {
    user = mkOption {
      type = str;
      default = "_circus-agent";
      description = "Daemon user the agent runs as.";
    };

    uid = mkOption {
      type = ints.positive;
      default = 531;
      description = "Numeric uid for the daemon user.";
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = !(cfg.settings.agent.rootless or false);
        message = "services.circus-agent: rootless mode is Linux-only";
      }
    ];

    users.users.${cfg.user} = {
      inherit (cfg) uid;
      home = toString cfg.settings.agent.work_dir;
      createHome = true;
      shell = "/usr/bin/false";
      description = "Circus build agent";
    };
    users.knownUsers = [cfg.user];

    system.activationScripts.circus-agent-workdir.text = ''
      install -d -m 0750 -o ${cfg.user} ${lib.escapeShellArg (toString cfg.settings.agent.work_dir)}
    '';

    launchd.daemons.circus-agent = {
      script = "exec ${startScript}";
      serviceConfig = {
        Label = "systems.manic.circus-agent";
        RunAtLoad = true;
        KeepAlive = true;
        UserName = cfg.user;
        WorkingDirectory = toString cfg.settings.agent.work_dir;
        StandardOutPath = "${toString cfg.settings.agent.work_dir}/stdout.log";
        StandardErrorPath = "${toString cfg.settings.agent.work_dir}/stderr.log";
        EnvironmentVariables = {
          PATH = "${lib.makeBinPath [pkgs.coreutils]}:${nixBin}";
          RUST_LOG = "info";
        };
      };
    };

    nix.settings = mkIf config.nix.enable {
      trusted-users = lib.mkAfter [cfg.user];
    };
  };
}
