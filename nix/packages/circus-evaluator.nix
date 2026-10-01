{
  craneLib,
  commonArgs,
  cargoArtifacts,
  gitMinimal,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-evaluator";
    cargoExtraArgs = "--package circus-evaluator";
    useNextest = true;
    # gix fetches file:// test remotes through `git upload-pack`.
    nativeCheckInputs = [gitMinimal];
  })
