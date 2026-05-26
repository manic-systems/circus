{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?ref=nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    crane,
    self,
    rust-overlay,
    ...
  }: let
    inherit (nixpkgs) lib;
    forAllSystems = lib.genAttrs ["x86_64-linux" "aarch64-linux"];
  in {
    # NixOS module for circus
    nixosModules = {
      circus = ./nix/modules/nixos.nix;
      default = self.nixosModules.circus;
    };

    packages = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system}.extend rust-overlay.overlays.default;
      craneLib = (crane.mkLib pkgs).overrideToolchain (p:
        # Build tools
        # We use the rust-overlay to get the stable Rust toolchain for various targets.
        # This is not exactly necessary, but it allows for compiling for various targets
        # with the least amount of friction.
          p.rust-bin.nightly.latest.default.override {
            extensions = ["rustfmt" "rust-analyzer" "clippy"];
            targets = [];
          });

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

      commonArgs = {
        pname = "circus";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with pkgs; [pkg-config];
        buildInputs = with pkgs; [openssl];
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in {
      demo-vm = pkgs.callPackage ./nix/demo-vm.nix {inherit self;};

      # circus Packages
      circus-common = pkgs.callPackage ./nix/packages/circus-common.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      circus-evaluator = pkgs.callPackage ./nix/packages/circus-evaluator.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      circus-migrate-cli = pkgs.callPackage ./nix/packages/circus-migrate-cli.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      circus-queue-runner = pkgs.callPackage ./nix/packages/circus-queue-runner.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      circus-server = pkgs.callPackage ./nix/packages/circus-server.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };
    });

    checks = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      vmTests = {
        # Split VM integration tests
        service-startup = pkgs.callPackage ./nix/tests/startup.nix {inherit self;};
        basic-api = pkgs.callPackage ./nix/tests/basic-api.nix {inherit self;};
        auth-rbac = pkgs.callPackage ./nix/tests/auth-rbac.nix {inherit self;};
        api-crud = pkgs.callPackage ./nix/tests/api-crud.nix {inherit self;};
        features = pkgs.callPackage ./nix/tests/features.nix {inherit self;};
        webhooks = pkgs.callPackage ./nix/tests/webhooks.nix {inherit self;};
        e2e = pkgs.callPackage ./nix/tests/e2e.nix {inherit self;};
        declarative = pkgs.callPackage ./nix/tests/declarative.nix {inherit self;};
        gc-pinning = pkgs.callPackage ./nix/tests/gc-pinning.nix {inherit self;};
        machine-health = pkgs.callPackage ./nix/tests/machine-health.nix {inherit self;};
        channel-tarball = pkgs.callPackage ./nix/tests/channel-tarball.nix {inherit self;};
      };
    in {
      inherit (vmTests) service-startup basic-api auth-rbac api-crud features webhooks e2e declarative gc-pinning machine-health channel-tarball;
      full = pkgs.symlinkJoin {
        name = "vm-tests-full";
        paths = builtins.attrValues vmTests;
      };
    });

    devShells = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      default = pkgs.mkShell {
        name = "fc-dev";
        inputsFrom = [self.packages.${system}.circus-server];

        strictDeps = true;
        packages = with pkgs; [
          pkg-config
          openssl
          postgresql_18

          taplo
          cargo-nextest
        ];
      };
    });

    formatter = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
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
          fd "$@" -t f -e md -x deno fmt -q '{}'
        '';
      });
  };
}
