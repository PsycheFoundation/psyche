{
  gitcommit,
  inputs,
  system,
  pkgs,
}:
let
  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    extensions = [ "rust-src" ];
    targets = [ "wasm32-unknown-unknown" ];
  };

  craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

  testResourcesFilter =
    path: _type:
    (builtins.match ".*tests/resources/.*$" path != null)
    || (builtins.match ".*.config/.*$" path != null)
    || (builtins.match ".*local-dev-keypair.json$" path != null);

  src = pkgs.lib.cleanSourceWith {
    src = ../.;
    filter = path: type: (testResourcesFilter path type) || (craneLib.filterCargoSources path type);
  };

  torch = pkgs.libtorch-bin.dev.overrideAttrs (
    old:
    let
      version = "2.6.0";
      cuda = "124";
    in
    {
      version = version;
      src = pkgs.fetchzip {
        name = "libtorch-cxx11-abi-shared-with-deps-${version}-cu${cuda}.zip";
        url = "https://download.pytorch.org/libtorch/cu${cuda}/libtorch-cxx11-abi-shared-with-deps-${version}%2Bcu${cuda}.zip";
        hash = "sha256-rJIvNGcR/xFfkr/O2a32CmO5/5Fv24Y7+efOtYgGO6A=";
      };
    }
  );

  env = {
    CUDA_ROOT = pkgs.cudaPackages.cudatoolkit.out;
    LIBTORCH = torch.out;
    LIBTORCH_INCLUDE = torch.dev;
    LIBTORCH_LIB = torch.out;
  };

  rustWorkspaceDeps = {
    nativeBuildInputs = with pkgs; [
      pkg-config
      perl
    ];

    buildInputs =
      [ torch ]
      ++ (with pkgs; [
        openssl
        fontconfig # for lr plot
      ])
      ++ (with pkgs.cudaPackages; [
        cudatoolkit
        cuda_cudart
        nccl
      ]);
  };

  rustWorkspaceArgs = rustWorkspaceDeps // {
    inherit env src;
    strictDeps = true;
  };

  cargoArtifacts = craneLib.buildDepsOnly rustWorkspaceArgs;

  buildRustPackage =
    {
      name,
      isExample ? false,
    }:
    craneLib.buildPackage (
      rustWorkspaceArgs
      // {
        inherit cargoArtifacts;
        pname = name;
        cargoExtraArgs = (if isExample then "--example ${name}" else "--bin ${name}");
        doCheck = false;
      }
    );

  # TODO: i can't set the rust build target to WASM for the build deps for wasm-pack, since *some* of them don't build.
  # really, i want like a wasm-only set of deps to build... can I do that?
  # like do the buildDepsOnly for not the workspace, but my specific package that *happens* to be in a workspace.
  buildRustWasmTsPackage =
    name:
    craneLib.buildPackage (
      rustWorkspaceArgs
      // {
        inherit cargoArtifacts;
        pname = name;
        doCheck = false;

        doNotPostBuildInstallCargoBinaries = true;

        nativeBuildInputs =
          rustWorkspaceArgs.nativeBuildInputs
          ++ (with pkgs; [
            wasm-pack
            jq
            wasm-bindgen-cli
          ]);

        buildPhaseCargoCommand = ''
          export CRATE_PATH=$(cargo metadata --format-version=1 --no-deps | jq -r ".packages[] | select(.name == \"${name}\") | .manifest_path" | xargs dirname)

          # wasm-pack needs a $HOME dir set.
          echo "building wasm"
          HOME=$TMPDIR wasm-pack build --target nodejs --mode no-install $CRATE_PATH

          echo "building ts bindings"
          cargo test -p ${name} export_bindings
        '';

        installPhase = ''
          mkdir -p $out

          pushd $CRATE_PATH
            # wasm-pack output
            if [ -d "pkg" ]; then
              cp -r pkg $out/
            fi

            # ts bindings
            if [ -d "bindings" ]; then
              cp -r bindings $out/
            fi
          popd
        '';
      }
    );

  buildWholeWorkspace = craneLib.buildPackage (
    rustWorkspaceArgs
    // {
      inherit cargoArtifacts;
    }
  );

  useHostGpuDrivers =
    package:
    pkgs.runCommand "${package.name}-nixgl-wrapped"
      {
        nativeBuildInputs = [ pkgs.makeWrapper ];
      }
      ''
        mkdir -p $out/bin
        for bin in ${package}/bin/*; do
          if [ -f "$bin" ] && [ -x "$bin" ]; then
            makeWrapper "$bin" "$out/bin/$(basename $bin)" \
              --run 'exec ${pkgs.nix-gl-host}/bin/nixglhost "'"$bin"'" -- "$@"'
          fi
        done
      '';

  solanaCraneLib =
    (inputs.crane.mkLib pkgs).overrideToolchain
      inputs.solana-pkgs.packages.${system}.solana-rust;

  # output the package's idl.json
  buildSolanaIdl =
    {
      src,
      programName,
      workspaceDir,
      sourceRoot,
      keypair ? "",
    }@origArgs:
    let
      args = builtins.removeAttrs origArgs [
        "keypair"
        "programName"
      ];
      cargoLock = workspaceDir + "/Cargo.lock";
      cargoToml = workspaceDir + "/Cargo.toml";

      env = {
        RUSTFLAGS = "--cfg procmacro2_semver_exempt -A warnings";
      };
      solanaWorkspaceArgs = rustWorkspaceDeps // {
        inherit
          env
          src
          sourceRoot
          cargoLock
          ;
      };
      solanaCargoArtifacts = solanaCraneLib.buildDepsOnly (
        solanaWorkspaceArgs
        // {
          pname = "solana-idl-${programName}";
          buildPhaseCargoCommand = "cargo test --no-run";
        }
      );
    in
    solanaCraneLib.mkCargoDerivation (
      solanaWorkspaceArgs
      // {
        cargoArtifacts = solanaCargoArtifacts;
        pname = programName;
        version = "0";
        pnameSuffix = "-idl";

        ANCHOR_IDL_BUILD_PROGRAM_PATH = "./programs/${programName}";

        postPatch =
          let
            cargoTomlContents = builtins.fromTOML (
              builtins.readFile (workspaceDir + "/programs" + "/${programName}" + "/Cargo.toml")
            );
          in
          ''
            if [ -n "${keypair}" ]; then
              mkdir -p ./target/deploy
              cp ${keypair} ./target/deploy/${cargoTomlContents.package.name}-keypair.json
            fi
          '';

        nativeBuildInputs = [
          inputs.solana-pkgs.packages.${system}.anchor
        ] ++ rustWorkspaceDeps.nativeBuildInputs;

        # TODO this doesn't reuse the build artifacts from the deps build, why not?
        buildPhaseCargoCommand = ''
          mkdir $out
          anchor idl build --out $out/idl.json --out-ts $out/idlType.ts
        '';

        doInstallCargoArtifacts = false;
      }
    );
in
{
  inherit
    rustToolchain
    craneLib
    buildSolanaIdl
    rustWorkspaceArgs
    cargoArtifacts
    buildRustPackage
    buildRustWasmTsPackage
    buildWholeWorkspace
    useHostGpuDrivers
    env
    src
    gitcommit
    ;
}
