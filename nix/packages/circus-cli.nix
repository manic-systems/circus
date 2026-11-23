{
  lib,
  stdenv,
  buildPackages,
  installShellFiles,
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

    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [installShellFiles];

    postInstall = lib.optionalString (stdenv.hostPlatform.emulatorAvailable buildPackages) (
      let
        circusctl = "${stdenv.hostPlatform.emulator buildPackages} $out/bin/circusctl";
      in ''
        installShellCompletion --cmd circusctl \
          --bash    <(COMPLETE=bash ${circusctl}) \
          --zsh     <(COMPLETE=zsh  ${circusctl}) \
          --fish    <(COMPLETE=fish ${circusctl}) \
          --nushell <(COMPLETE=nu   ${circusctl})
      ''
    );
  })
