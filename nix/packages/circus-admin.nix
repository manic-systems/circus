{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-admin";
    cargoExtraArgs = "--package circus-admin";
    useNextest = true;
  })
