# Evaluates the `herculesCI` flake attribute the way hercules-ci-agent does
# and returns `{ <job> = <outputs>; }` for evix to walk.
{circusHerculesJson}: let
  ctx = builtins.fromJSON circusHerculesJson;
  flake = builtins.getFlake ctx.flakeRef;
  outputs = flake.outputs;
  hci = outputs.herculesCI or {};
  primaryRepo = ctx.primaryRepo // {outPath = flake.outPath;};
  args = {
    inherit (primaryRepo) ref branch tag rev shortRev;
    inherit primaryRepo;
    herculesCI = ctx.herculesCI;
  };
  value =
    if builtins.isFunction hci
    then hci args
    else hci;
  ciSystems = value.ciSystems or null;
  keep = system: ciSystems == null || builtins.elem system ciSystems;
  perSystem = name: let
    bySystem = outputs.${name} or {};
  in
    builtins.listToAttrs (map (system: {
      name = system;
      value = bySystem.${system};
    }) (builtins.filter keep (builtins.attrNames bySystem)));
  # Hercules generates this job when a flake declares no onPush.
  defaultJob.outputs = {
    packages = perSystem "packages";
    checks = perSystem "checks";
    devShells = perSystem "devShells";
    effects = outputs.effects or {};
  };
in
  builtins.mapAttrs (_: job: job.outputs) (value.onPush or {default = defaultJob;})
