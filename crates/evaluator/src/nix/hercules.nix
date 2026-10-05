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
  onSchedule = value.onSchedule or {};
  # evix only reports derivations, so the schedules ride on one whose meta the
  # evaluator reads and drops before creating builds.
  scheduleMarker =
    derivation {
      name = "circus-schedules";
      system = "builtin";
      builder = "builtin:circus-schedules";
    }
    // {
      meta.description =
        builtins.toJSON (builtins.mapAttrs (_: job: job.when or {}) onSchedule);
    };
in
  if ctx.schedule == null
  then
    builtins.mapAttrs (_: job: job.outputs) (value.onPush or {default = defaultJob;})
    // {${ctx.scheduleMarker} = scheduleMarker;}
  else {${ctx.schedule} = onSchedule.${ctx.schedule}.outputs;}
