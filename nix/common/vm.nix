{
  # Common machine configuration for all circus integration tests
  imports = [./base.nix];

  config = {
    virtualisation = {
      memorySize = 2048;
      cores = 2;
      diskSize = 10000;
      graphics = false;

      # Forward guest:3000 -> host:3000 so the dashboard is reachable
      forwardPorts = [
        {
          from = "host";
          host.port = 3000;
          guest.port = 3000;
        }
      ];
    };
  };
}
