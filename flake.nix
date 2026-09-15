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
    forAllSystems = lib.genAttrs (lib.systems.doubles.linux ++ ["aarch64-darwin"]);
    pkgsFor = system: nixpkgs.legacyPackages.${system} or (import nixpkgs {inherit system;});
    src = let
      fs = lib.fileset;
      s = ./.;
    in
      fs.toSource {
        root = s;
        fileset = fs.unions [
          (s + /crates)
          (s + /db/circus-codegen)
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
          (fs.fileFilter (file: file.name == "Cargo.toml") (s + /db/circus-codegen))
        ];
      };
    mkAgent = pkgs: let
      craneLib = crane.mkLib pkgs;
      # The agent also builds on Darwin, so it skips the control plane's
      # Nix/glibc/libclang stack.
      commonArgs = {
        pname = "circus-agent";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with pkgs.buildPackages; [pkg-config capnproto];
        buildInputs = [];
      };
      cargoArtifacts = craneLib.buildDepsOnly (commonArgs
        // {
          src = cargoDepsSrc;
          cargoExtraArgs = "--package circus-agent";
        });
    in {
      inherit cargoArtifacts;
      package = pkgs.callPackage ./nix/packages/circus-agent.nix {
        inherit craneLib cargoArtifacts commonArgs;
      };
    };
    mkPackageSet = pkgs: let
      craneLib = crane.mkLib pkgs;
      agent = mkAgent pkgs;
      # circus-evaluator embeds evix, which builds Rust bindings against the
      # Nix C API (via nix-bindings-sys). Building the workspace dependency
      # closure therefore needs the Nix C dev libraries, a glibc sysroot, and
      # libclang for bindgen.
      commonArgs = {
        pname = "circus";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with pkgs.buildPackages; [pkg-config capnproto];
        buildInputs = with pkgs; [openssl sqlite nixVersions.nix_2_34.dev] ++ lib.optionals stdenv.hostPlatform.isLinux [glibc.dev];
        env = {
          LIBCLANG_PATH = "${pkgs.buildPackages.llvmPackages.libclang.lib}/lib";
          BINDGEN_EXTRA_CLANG_ARGS = lib.optionalString pkgs.stdenv.hostPlatform.isLinux "--sysroot=${pkgs.glibc.dev}";
        };
      };
      depsCommonArgs = commonArgs // {src = cargoDepsSrc;};
      cargoArtifacts = craneLib.buildDepsOnly depsCommonArgs;
      # Kept out of commonArgs so the shared dependency artifacts stay cached across commits
      buildShaArgs = {
        env = commonArgs.env // {CIRCUS_BUILD_SHA = self.rev or self.dirtyRev or "";};
      };
      callCratePackage = path:
        pkgs.callPackage path {
          inherit craneLib cargoArtifacts;
          commonArgs = commonArgs // buildShaArgs;
        };
    in {
      inherit craneLib cargoArtifacts depsCommonArgs;
      agentCargoArtifacts = agent.cargoArtifacts;
      packages = {
        circus-cli = callCratePackage ./nix/packages/circus-cli.nix;
        circus-agent = agent.package;
        circus-evaluator = callCratePackage ./nix/packages/circus-evaluator.nix;
        circus-queue-runner = callCratePackage ./nix/packages/circus-queue-runner.nix;
        circus-server = callCratePackage ./nix/packages/circus-server.nix;
      };
    };
  in {
    lib.mkPackages = pkgs: (mkPackageSet pkgs).packages;

    # NixOS modules for Circus and components
    nixosModules = {
      circus = {
        _file = ./flake.nix;
        key = "circus/nixosModules/circus";
        imports = [
          ./nix/modules/circus.nix
          ({pkgs, ...}: let
            packages = self.lib.mkPackages pkgs;
          in {
            services.circus = {
              package = lib.mkDefault packages.circus-server;
              evaluatorPackage = lib.mkDefault packages.circus-evaluator;
              queueRunnerPackage = lib.mkDefault packages.circus-queue-runner;
              migratePackage = lib.mkDefault packages.circus-cli;
            };
          })
        ];
      };
      circus-agent = {
        _file = ./flake.nix;
        key = "circus/nixosModules/circus-agent";
        imports = [
          ./nix/modules/circus-agent.nix
          ({pkgs, ...}: {
            services.circus-agent.package =
              lib.mkDefault (self.lib.mkPackages pkgs).circus-agent;
          })
        ];
      };
      default = self.nixosModules.circus; # agent is optional
    };

    darwinModules = {
      circus-agent = {
        _file = ./flake.nix;
        key = "circus/darwinModules/circus-agent";
        imports = [
          ./nix/modules/circus-agent-darwin.nix
          ({pkgs, ...}: {
            services.circus-agent.package =
              lib.mkDefault (self.lib.mkPackages pkgs).circus-agent;
          })
        ];
      };
      default = self.darwinModules.circus-agent;
    };

    packages = forAllSystems (system: let
      pkgs = pkgsFor system;
      packageSet = mkPackageSet pkgs;
      inherit
        (packageSet)
        agentCargoArtifacts
        cargoArtifacts
        craneLib
        depsCommonArgs
        ;

      muslCrossAttr = {
        x86_64-linux = "musl64";
        i686-linux = "musl32";
        aarch64-linux = "aarch64-multiplatform-musl";
        armv6l-linux = "muslpi";
        powerpc64-linux = "ppc64-musl";
        riscv64-linux = "riscv64-musl";
      };

      # A statically linked agent
      crossPkgs = pkgs.pkgsCross.${muslCrossAttr.${system}};
      staticCraneLib = crane.mkLib crossPkgs;
      staticAgentArgs = {
        pname = "circus-agent-static";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with crossPkgs.buildPackages; [pkg-config capnproto];
        buildInputs = [(crossPkgs.openssl.override {static = true;})];
        cargoExtraArgs = "--package circus-agent";
        doCheck = false;
        hardeningDisable = ["fortify" "fortify3"];
        env.CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
      };
      staticCargoArtifacts = staticCraneLib.buildDepsOnly staticAgentArgs;
    in
      packageSet.packages
      // {
        demo-vm = pkgs.callPackage ./nix/demo-vm.nix {inherit self;};

        ci-cargo-artifacts = pkgs.linkFarm "ci-cargo-artifacts" ([
            {
              name = "deps";
              path = cargoArtifacts;
            }
            {
              name = "vendor";
              path = craneLib.vendorCargoDeps depsCommonArgs;
            }
            {
              name = "deps-agent";
              path = agentCargoArtifacts;
            }
          ]
          ++ lib.optionals (muslCrossAttr ? ${system}) [
            {
              name = "deps-static";
              path = staticCargoArtifacts;
            }
          ]);
      }
      // lib.optionalAttrs (muslCrossAttr ? ${system}) {
        circus-agent-static = staticCraneLib.buildPackage (
          staticAgentArgs
          // {
            cargoArtifacts = staticCargoArtifacts;
            env = staticAgentArgs.env // {CIRCUS_BUILD_SHA = self.rev or self.dirtyRev or "";};
          }
        );
      });

    checks = forAllSystems (system: let
      pkgs = pkgsFor system;
      craneLib = crane.mkLib pkgs;

      callTest = path: pkgs.callPackage path {inherit self;};
      nixosTests = {
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
        channel-tarball = callTest ./nix/tests/channel-tarball.nix;
        distributed = callTest ./nix/tests/distributed.nix;
        agent-dispatch = callTest ./nix/tests/agent-dispatch.nix;
        capability-scheduling = callTest ./nix/tests/capability-scheduling.nix;
        s3-cache = callTest ./nix/tests/s3-cache.nix;
        caches = callTest ./nix/tests/caches.nix;
      };
      filteredNixosTests = lib.filterAttrs (_: t: builtins.length (lib.attrNames t.config.containers) > 0 -> pkgs.stdenv.hostPlatform.isLinux) nixosTests;
    in
      filteredNixosTests
      // {
        full = pkgs.symlinkJoin {
          name = "nixos-tests-full";
          paths = builtins.attrValues filteredNixosTests;
        };

        cargo-deny = craneLib.cargoDeny {
          pname = "circus-audit";
          src = let
            fs = lib.fileset;
            s = ./.;
          in
            fs.toSource {
              root = s;
              fileset = fs.unions [
                (s + /crates)
                (s + /db/circus-codegen)
                (s + /Cargo.lock)
                (s + /Cargo.toml)
                (s + /.deny.toml)
              ];
            };
          cargoDenyChecks = "bans licenses sources";
          cargoExtraArgs = "--locked";
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

        # Keep checked-in bindings synchronized with queries and migrations.
        codegen-up-to-date =
          pkgs.runCommand "circus-codegen-up-to-date" {
            nativeBuildInputs = [pkgs.postgresql_18 pkgs.cornucopia pkgs.rustfmt];
          } ''
            export PGDATA="$TMPDIR/pgdata"
            export PGHOST="$TMPDIR/sock"
            mkdir -p "$PGHOST"
            initdb -D "$PGDATA" -U postgres --auth=trust --no-sync >/dev/null
            pg_ctl -D "$PGDATA" \
              -o "-k $PGHOST -c listen_addresses=''' -c fsync=off" -w start >/dev/null
            createdb -h "$PGHOST" -U postgres circus_check
            for f in ${./crates/migrations/migrations}/[0-9]*.sql; do
              psql -v ON_ERROR_STOP=1 -h "$PGHOST" -U postgres -d circus_check -q -f "$f"
            done
            mkdir -p "$TMPDIR/work"
            cp -r ${./queries} "$TMPDIR/work/queries"
            cp ${./cornucopia.toml} "$TMPDIR/work/cornucopia.toml"
            psql -v ON_ERROR_STOP=1 -h "$PGHOST" -U postgres -d circus_check -q -f ${./crates/migrations/bootstrap.sql}
            (cd "$TMPDIR/work" && cornucopia live "host=$PGHOST user=postgres dbname=circus_check")
            pg_ctl -D "$PGDATA" -m immediate stop >/dev/null || true
            if ! diff -ru ${./db/circus-codegen} "$TMPDIR/work/db/circus-codegen"; then
              echo "ERROR: db/circus-codegen/ is out of date relative to queries/ x migrations/." >&2
              echo "Run scripts/codegen.sh and commit the regenerated db/circus-codegen/." >&2
              exit 1
            fi
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
        packages = with pkgs;
          [
            pkg-config
            openssl
            postgresql_18
            # DB query codegen: `scripts/codegen.sh` runs `cornucopia live`.
            cornucopia

            # circus-evaluator builds evix's Nix C bindings.
            nixVersions.nix_2_34.dev

            taplo
            cargo-deny
            cargo-nextest
            clippy
            rust-analyzer
            (rustfmt.override {asNightly = true;})
          ]
          ++ lib.optionals stdenv.hostPlatform.isLinux [glibc.dev];

        # bindgen (via nix-bindings-sys) needs libclang and a glibc sysroot.
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        BINDGEN_EXTRA_CLANG_ARGS = lib.optionalString pkgs.stdenv.hostPlatform.isLinux "--sysroot=${pkgs.glibc.dev}";
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

          # Format TOML with Taplo, leaving the generated codegen crate verbatim
          fd "$@" -t f -e toml -E db -x taplo fmt '{}'

          # Format CSS with Prettier
          fd "$@" -t f -e css -x prettier --write '{}'

          # Format SQL with sql-format, skipping cornucopia queries it cannot parse
          fd "$@" -t f -e sql -E queries -x sql-formatter --fix '{}' -l postgresql

          # Format Markdown with Deno
          fd "$@" -t f -e md -E docs/API.md -x deno fmt -q '{}'
        '';
      });
  };
}
