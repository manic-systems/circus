{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-remote-cache";
    cargoExtraArgs = "--package circus-remote-cache";
    useNextest = true;
  })
