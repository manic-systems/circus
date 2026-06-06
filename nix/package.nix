{
  lib,
  rustPlatform,
  pkg-config,
  capnproto,
  openssl,
  crate ? "circus-agent",
}:
rustPlatform.buildRustPackage {
  pname = crate;
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../crates
      ../Cargo.toml
      ../Cargo.lock
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = ["--package" crate];
  cargoTestFlags = ["--package" crate];

  nativeBuildInputs = [pkg-config capnproto];
  buildInputs = [openssl];

  meta.mainProgram = crate;
}
