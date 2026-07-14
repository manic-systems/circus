{
  lib,
  rustPlatform,
  pkg-config,
  capnproto,
  openssl,
  crate ? "circus-agent",
}: let
  cargoTOML = (lib.importTOML ../Cargo.toml).workspace.package;
in
  rustPlatform.buildRustPackage (finalAttrs: {
    pname = crate;
    version = cargoTOML.version;

    src = let
      fs = lib.fileset;
      s = ./..;
    in
      fs.toSource {
        root = s;
        fileset = fs.unions [
          (s + /crates)
          (s + /db/circus-codegen)
          (s + /Cargo.toml)
          (s + /Cargo.lock)
        ];
      };

    cargoLock.lockFile = "${finalAttrs.src}/Cargo.lock";

    cargoBuildFlags = ["--package" crate];
    cargoTestFlags = ["--package" crate];
    useNextest = true;

    nativeBuildInputs = [pkg-config capnproto];
    buildInputs = [openssl.dev];
    meta = {
      homepage = "https://github.com/manic-systems/circus";
      mainProgram = crate;
      maintainers = with lib.maintainers; [NotAShelf];
    };
  })
