{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-cli";
    cargoExtraArgs = "--package circus-cli --bin circusctl";
    useNextest = true;
  })
