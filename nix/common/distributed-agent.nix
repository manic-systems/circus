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
  fixtureToken = "demo-agent-token-${name}-please-rotate";
  fixtureMachineId = "00000000-0000-4000-8000-${builtins.substring 0 12 (builtins.hashString "sha256" name)}";
in {
  imports = [self.nixosModules.circus-agent];

  environment.systemPackages = with pkgs; [nix curl jq];
  nix = {
    settings.experimental-features = ["nix-command" "flakes"];
    settings.substituters = lib.mkForce [];
  };

  environment.etc."circus-agent/token".text = fixtureToken;

  services.circus-agent = {
    enable = true;
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
      machine_id_file = toString (pkgs.writeText "circus-agent-machine-id-${name}" fixtureMachineId);
    };
  };
}
