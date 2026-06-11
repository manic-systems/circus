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
  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "harmonia-file-nar-3.1.0" = "sha256-6LJOkuyWuMjENbzZCKDOjEz4qjYipTwH0qRMcwpdLSk=";
    };
  };

  cargoBuildFlags = ["--package" crate];
  cargoTestFlags = ["--package" crate];

  nativeBuildInputs = [pkg-config capnproto];
  buildInputs = [openssl];

  meta.mainProgram = crate;
}
