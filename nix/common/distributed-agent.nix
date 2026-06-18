{
  self,
  name ? "agent-01",
  features ? [],
  maxJobs ? 2,
  speed ? 1.0,
}: {
  pkgs,
  lib,
  ...
}: let
  circus-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
in {
  imports = [self.nixosModules.circus-agent];

  environment.systemPackages = with pkgs; [nix curl jq];
  nix = {
    settings.experimental-features = ["nix-command" "flakes"];
    settings.substituters = lib.mkForce [];
  };

  environment.etc."circus-agent/token".text = "demo-agent-token-please-rotate";

  services.circus-agent = {
    enable = true;
    package = circus-packages.circus-agent;
    authTokenFile = "/etc/circus-agent/token";
    settings.agent = {
      inherit name;
      runner_url = "circus://runner:8443";
      systems = [pkgs.stdenv.hostPlatform.system];
      supported_features = features;
      max_jobs = maxJobs;
      speed_factor = speed;
      heartbeat_interval_secs = 3;
      reconnect_delay_secs = 2;
    };
  };
}
