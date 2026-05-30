{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-agent";
    cargoExtraArgs = "--package circus-agent";
    useNextest = true;
  })
