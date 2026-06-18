{
  # Common machine configuration for circus integration tests that run as
  # nspawn containers rather than QEMU VMs. Identical to vm-common.nix but
  # without the virtualisation.* options that only exist in qemu-vm.nix.
  imports = [./base.nix];
}
