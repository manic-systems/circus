{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-migrate-cli";
    cargoExtraArgs = "--package circus-migrate-cli";
  })
