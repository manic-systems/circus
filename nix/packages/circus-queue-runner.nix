{
  craneLib,
  commonArgs,
  cargoArtifacts,
  cacert,
}:
craneLib.buildPackage (commonArgs
  // {
    inherit cargoArtifacts;
    pname = "circus-queue-runner";
    cargoExtraArgs = "--package circus-queue-runner";
    useNextest = true;
    nativeBuildInputs = commonArgs.nativeBuildInputs or [] ++ [cacert];
    env = commonArgs.env or {} // {SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";};
  })
