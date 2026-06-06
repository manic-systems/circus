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

      commonArgs = {
        pname = "circus";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with pkgs; [pkg-config capnproto];
        buildInputs = with pkgs; [openssl];
      };

      # agent doesn't need openssl
      agentArgs = commonArgs // {buildInputs = [];};

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      agentArtifacts = craneLib.buildDepsOnly (agentArgs
        // {
          pname = "circus-agent";
          cargoExtraArgs = "--package circus-agent";
        });

      callCratePackage = path: pkgs.callPackage path {inherit craneLib commonArgs cargoArtifacts;};
    in {
      demo-vm = pkgs.callPackage ./nix/demo-vm.nix {inherit self;};

      # circus Packages
      circus-admin = callCratePackage ./nix/packages/circus-admin.nix;
      circus-agent = (callCratePackage ./nix/packages/circus-agent.nix).override {
        commonArgs = agentArgs;
        cargoArtifacts = agentArtifacts;
      };
      circus-evaluator = callCratePackage ./nix/packages/circus-evaluator.nix;
      circus-migrate-cli = callCratePackage ./nix/packages/circus-migrate-cli.nix;
      circus-queue-runner = callCratePackage ./nix/packages/circus-queue-runner.nix;
      circus-server = callCratePackage ./nix/packages/circus-server.nix;
    });

    checks = forAllSystems (system: let
      pkgs = pkgsFor system;

      callTest = path: pkgs.callPackage path {inherit self;};

      formatting = pkgs.runCommand "circus-formatting-check" {nativeBuildInputs = [self.formatter.${system}];} ''
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
        inherit formatting;
        full = pkgs.symlinkJoin {
          name = "vm-tests-full";
          paths = builtins.attrValues vmTests;
        };
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

          taplo
          cargo-nextest
          clippy
          rust-analyzer
          (rustfmt.override {asNightly = true;})
        ];
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
