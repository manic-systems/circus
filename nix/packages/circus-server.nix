{
  craneLib,
  commonArgs,
  cargoArtifacts,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-server";
    cargoExtraArgs = "--package circus-server";
    useNextest = true;

    # The bundled browser assets are read from vendored sources, which only
    # exist at build time.
    postInstall = ''
      $out/bin/circus-server --bundle-assets $out/share/circus-server/assets
    '';
  })
