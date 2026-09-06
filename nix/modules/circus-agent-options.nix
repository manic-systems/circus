self: {
  pkgs,
  lib,
  ...
}: let
  inherit (lib.options) mkOption mkEnableOption mkPackageOption;
  inherit (lib.types) listOf package str path ints submodule;
  settingsFormat = pkgs.formats.toml {};

  selfPkgs = self.packages.${pkgs.stdenv.hostPlatform.system};
in {
  options.services.circus-agent = {
    enable = mkEnableOption "Circus distributed build agent";

    package = mkPackageOption selfPkgs "circus-agent" {};

    authTokenFile = mkOption {
      type = path;
      description = ''
        Path to a file containing the bearer token. The token is rendered
        into a runtime config private to the service and never lands in
        the Nix store.
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

              cores = mkOption {
                type = ints.unsigned;
                default = 0;
                description = "Per-build nix `cores` cap. 0 keeps the host's default.";
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
}
