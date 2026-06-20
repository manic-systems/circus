{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?ref=nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    nixpkgs,
    crane,
    self,
    ...
  }: let
    inherit (nixpkgs) lib;
    forAllSystems = lib.genAttrs lib.systems.doubles.linux;
    pkgsFor = system: nixpkgs.legacyPackages.${system} or (import nixpkgs {inherit system;});
  in {
    # NixOS modules for Circus and components
    nixosModules = {
      circus = ./nix/modules/circus.nix;
      circus-agent = ./nix/modules/circus-agent.nix;
      default = self.nixosModules.circus; # agent is optional
    };

    packages = forAllSystems (system: let
      pkgs = pkgsFor system;
      craneLib = crane.mkLib pkgs;
      src = let
        fs = lib.fileset;
        s = ./.;
      in
        fs.toSource {
          root = s;
          fileset = fs.unions [
            (s + /crates)
            (s + /Cargo.lock)
            (s + /Cargo.toml)
          ];
        };

      cargoDepsSrc = let
        fs = lib.fileset;
        s = ./.;
      in
        fs.toSource {
          root = s;
          fileset = fs.unions [
            (s + /Cargo.lock)
            (s + /Cargo.toml)
            (fs.fileFilter (file: file.name == "Cargo.toml") (s + /crates))
          ];
        };

      # circus-evaluator embeds evix, which builds Rust bindings against the
      # Nix C API (via nix-bindings-sys). Building the workspace dependency
      # closure therefore needs the Nix C dev libraries, a glibc sysroot, and
      # libclang for bindgen.
      commonArgs = {
        pname = "circus";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with pkgs; [pkg-config capnproto];
        buildInputs = with pkgs; [openssl nixVersions.nix_2_34.dev glibc.dev];
        env = {
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${pkgs.glibc.dev}";
        };
      };

      depsCommonArgs = commonArgs // {src = cargoDepsSrc;};
      cargoArtifactsFor = name: cargoExtraArgs:
        craneLib.buildDepsOnly (depsCommonArgs
          // {
            pname = name;
            inherit cargoExtraArgs;
          });

      callCratePackage = path: name: cargoExtraArgs:
        pkgs.callPackage path {
          inherit craneLib commonArgs;
          cargoArtifacts = cargoArtifactsFor name cargoExtraArgs;
        };

      muslCrossAttr = {
        x86_64-linux = "musl64";
        i686-linux = "musl32";
        aarch64-linux = "aarch64-multiplatform-musl";
        armv6l-linux = "muslpi";
        powerpc64-linux = "ppc64-musl";
        riscv64-linux = "riscv64-musl";
      };

      # A statically linked agent
      crossPkgs = pkgs.pkgsCross.${muslCrossAttr.${system} or "musl64"};
      staticCraneLib = crane.mkLib crossPkgs;
      staticAgentArgs = {
        pname = "circus-agent-static";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with crossPkgs.buildPackages; [pkg-config capnproto];
        buildInputs = [crossPkgs.openssl];
        cargoExtraArgs = "--package circus-agent";
        doCheck = false;
        CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        hardeningDisable = ["fortify" "fortify3"];
      };
    in
      {
        demo-vm = pkgs.callPackage ./nix/demo-vm.nix {inherit self;};

        # circus Packages
        circus-cli = callCratePackage ./nix/packages/circus-cli.nix "circus-cli" "--package circus-cli --bin circusctl";
        circus-agent = callCratePackage ./nix/packages/circus-agent.nix "circus-agent" "--package circus-agent";
        circus-evaluator = callCratePackage ./nix/packages/circus-evaluator.nix "circus-evaluator" "--package circus-evaluator";
        circus-queue-runner = callCratePackage ./nix/packages/circus-queue-runner.nix "circus-queue-runner" "--package circus-queue-runner";
        circus-server = callCratePackage ./nix/packages/circus-server.nix "circus-server" "--package circus-server";
      }
      // lib.optionalAttrs (muslCrossAttr ? ${system}) {
        circus-agent-static = staticCraneLib.buildPackage (
          staticAgentArgs // {cargoArtifacts = staticCraneLib.buildDepsOnly staticAgentArgs;}
        );
      });

    checks = forAllSystems (system: let
      pkgs = pkgsFor system;

      callTest = path: pkgs.callPackage path {inherit self;};
      nixosModuleAgentPackage = pkgs.callPackage ./nix/package.nix {crate = "circus-agent";};
      vmTests = {
        # Split VM integration tests
        service-startup = callTest ./nix/tests/startup.nix;
        basic-api = callTest ./nix/tests/basic-api.nix;
        auth-rbac = callTest ./nix/tests/auth-rbac.nix;
        api-crud = callTest ./nix/tests/api-crud.nix;
        features = callTest ./nix/tests/features.nix;
        webhooks = callTest ./nix/tests/webhooks.nix;
        e2e = callTest ./nix/tests/e2e.nix;
        declarative = callTest ./nix/tests/declarative.nix;
        gc-pinning = callTest ./nix/tests/gc-pinning.nix;
        machine-health = callTest ./nix/tests/machine-health.nix;
        channel-tarball = callTest ./nix/tests/channel-tarball.nix;
        distributed = callTest ./nix/tests/distributed.nix;
        agent-dispatch = callTest ./nix/tests/agent-dispatch.nix;
        capability-scheduling = callTest ./nix/tests/capability-scheduling.nix;
        s3-cache = callTest ./nix/tests/s3-cache.nix;
      };
    in
      vmTests
      // {
        nixos-module-agent-package = nixosModuleAgentPackage;
        full = pkgs.symlinkJoin {
          name = "vm-tests-full";
          paths = builtins.attrValues vmTests;
        };

        formatting =
          pkgs.runCommand "circus-formatting-check" {
            preferLocal = true;
            nativeBuildInputs = [self.formatter.${system}];
          } ''
            cp -r --no-preserve=mode ${self} src
            cd src
            export HOME="$TMPDIR" DENO_DIR="$TMPDIR/deno"
            nix3-fmt-wrapper
            diff -ru ${self} . || {
              echo "::error::Tree is not formatted; run 'nix fmt' and commit the result." >&2
              exit 1
            }
            touch "$out"
          '';
      });

    devShells = forAllSystems (system: let
      pkgs = pkgsFor system;
    in {
      default = pkgs.mkShell {
        name = "circus-dev";
        inputsFrom = [self.packages.${system}.circus-server];

        strictDeps = true;
        packages = with pkgs; [
          pkg-config
          openssl
          postgresql_18

          # circus-evaluator builds evix's Nix C bindings.
          nixVersions.nix_2_34.dev
          glibc.dev

          taplo
          cargo-nextest
          clippy
          rust-analyzer
          (rustfmt.override {asNightly = true;})
        ];

        # bindgen (via nix-bindings-sys) needs libclang and a glibc sysroot.
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${pkgs.glibc.dev}";
      };
    });

    formatter = forAllSystems (system: let
      pkgs = pkgsFor system;
    in
      pkgs.writeShellApplication {
        name = "nix3-fmt-wrapper";

        runtimeInputs = [
          pkgs.alejandra
          pkgs.fd
          pkgs.prettier
          pkgs.deno
          pkgs.taplo
          pkgs.sql-formatter
        ];

        text = ''
          # Format Nix with Alejandra
          fd "$@" -t f -e nix -x alejandra -q '{}'

          # Format TOML with Taplo
          fd "$@" -t f -e toml -x taplo fmt '{}'

          # Format CSS with Prettier
          fd "$@" -t f -e css -x prettier --write '{}'

          # Format SQL with sql-format
          fd "$@" -t f -e sql -x sql-formatter --fix '{}' -l postgresql

          # Format Markdown with Deno
          fd "$@" -t f -e md -E docs/API.md -x deno fmt -q '{}'
        '';
      });
  };
}
