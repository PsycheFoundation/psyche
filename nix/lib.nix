{
  pkgs,
  inputs,
  pythonSet,
  lib ? pkgs.lib,
  gitcommit ? inputs.self.rev or inputs.self.dirtyRev or "unknown",
  system ? pkgs.stdenv.hostPlatform.system,
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
    || (builtins.match ".*tests/fixtures/.*$" path != null)
    || (builtins.match ".*.config/.*$" path != null)
    || (builtins.match ".*local-dev-keypair.json$" path != null)
    || (builtins.match ".*shared/client/src/state/prompt_texts/index\\.json$" path != null);

  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type: (testResourcesFilter path type) || (craneLib.filterCargoSources path type);
  };

  env = {
    LIBTORCH_USE_PYTORCH = 1;
  };

  # build-time python environment with all uv2nix deps + nixpkgs pytorch
  basePythonEnv = pythonSet.mkVirtualEnv "psyche-build-env" {
    psyche = [ ];
  };

  rustWorkspaceDeps = {
    nativeBuildInputs = with pkgs; [
      pkg-config
      perl
    ];

    buildInputs = [
      basePythonEnv
    ]
    ++ (with pkgs; [
      openssl
      fontconfig # for lr plot
    ])
    ++ lib.optionals pkgs.config.cudaSupport (
      with pkgs.cudaPackages;
      [
        cudatoolkit
        cuda_cudart
        nccl
      ]
      ++ (with pkgs; [
        rdma-core
      ])
    );
  };

  rustWorkspaceArgs = rustWorkspaceDeps // {
    inherit env src;
    strictDeps = true;
    # Enable parallelism feature only on CUDA-supported platforms
    cargoExtraArgs = "--features python" + lib.optionalString (pkgs.config.cudaSupport) ",parallelism";
  };

  rustWorkspaceArgsWithPython = rustWorkspaceArgs // {
    buildInputs = rustWorkspaceArgs.buildInputs ++ [
      pythonWithPsycheExtension
    ];
    NIX_LDFLAGS = "-L${pythonWithPsycheExtension}/lib -lpython3.12";
  };

  rustWorkspaceArgsNoPython = rustWorkspaceDeps // {
    inherit env src;
    strictDeps = true;
    # Enable parallelism feature only on CUDA-supported platforms
    cargoExtraArgs = lib.optionalString (pkgs.config.cudaSupport) "--features parallelism";
  };

  cargoArtifacts = craneLib.buildDepsOnly rustWorkspaceArgs;
  cargoArtifactsNoPython = craneLib.buildDepsOnly rustWorkspaceArgsNoPython;

  # Build the rust extension separately
  rustExtension = craneLib.buildPackage (
    rustWorkspaceArgs
    // {
      inherit cargoArtifacts;
      pname = "psyche-python-extension";
      cargoExtraArgs =
        " --package psyche-python-extension"
        + lib.optionalString (pkgs.config.cudaSupport) " --features parallelism";
      doCheck = false;
    }
  );

  # Runtime python environment = build-time env + rust extension
  pythonWithPsycheExtension =
    let
      ext = if pkgs.stdenv.isDarwin then "dylib" else "so";

      # Package that just provides the rust extension .so file
      psycheExtensionPackage = pkgs.runCommand "psyche-extension" { } ''
        mkdir -p $out/${basePythonEnv}/lib/python3.12/site-packages/psyche
        cp ${rustExtension}/lib/lib${builtins.replaceStrings [ "-" ] [ "_" ] rustExtension.pname}.${ext} \
           $out/${basePythonEnv}/lib/python3.12/site-packages/psyche/_psyche_ext.so

        # Create __init__.py to make it importable
        echo 'from ._psyche_ext import *' > $out/${basePythonEnv}/lib/python3.12/site-packages/psyche/__init__.py
      '';
    in
    basePythonEnv;

  buildRustPackageWithPsychePythonEnvironment =
    {
      name,
      isExample ? false,
    }:
    let
      rustPackage = craneLib.buildPackage (
        rustWorkspaceArgsWithPython
        // {
          inherit cargoArtifacts;
          pname = name;
          cargoExtraArgs =
            rustWorkspaceArgsWithPython.cargoExtraArgs
            + (if isExample then " --example ${name}" else " --bin ${name}");
          doCheck = false;

          meta.mainProgram = name;
        }
      );
    in
    pkgs.runCommand "${name}"
      {
        buildInputs = [ pkgs.makeWrapper ];
      }
      ''
        mkdir -p $out/bin
        makeWrapper ${rustPackage}/bin/${name} $out/bin/${name} \
          --prefix PATH : "${pythonWithPsycheExtension}/bin"
      '';

  # --set PYTHONPATH "${pythonWithPsycheExtension}/${pythonWithPsycheExtension.sitePackages}" \

  buildRustPackageWithoutPython =
    {
      name,
      isExample ? false,
    }:
    craneLib.buildPackage (
      rustWorkspaceArgsNoPython
      // {
        cargoArtifacts = cargoArtifactsNoPython;
        pname = name;
        cargoExtraArgs =
          rustWorkspaceArgsNoPython.cargoExtraArgs
          + (if isExample then " --example ${name}" else " --bin ${name}");
        doCheck = false;

        meta.mainProgram = name;
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
        cargoExtraArgs = ""; # *remove* features - we don't want the cuda stuff in here.
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

  useHostGpuDrivers =
    if pkgs.config.cudaSupport then
      (
        package:
        pkgs.runCommandNoCC "${package.name}-nixgl-wrapped"
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
          ''
      )
    else
      (package: package);

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
    }:
    let
      cargoLock = workspaceDir + "/Cargo.lock";

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
          buildPhaseCargoCommand = "cargo test --no-run --features idl-build";
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
            cargoTomlContents = lib.importTOML (workspaceDir + "/programs/${programName}/Cargo.toml");
          in
          ''
            if [ -n "${keypair}" ]; then
              mkdir -p ./target/deploy
              cp ${keypair} ./target/deploy/${cargoTomlContents.package.name}-keypair.json
            fi
          '';

        nativeBuildInputs = [
          inputs.solana-pkgs.packages.${system}.anchor
        ]
        ++ rustWorkspaceDeps.nativeBuildInputs;

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
    rustWorkspaceArgsWithPython
    cargoArtifacts
    buildRustPackageWithPsychePythonEnvironment
    buildRustPackageWithoutPython
    buildRustWasmTsPackage
    useHostGpuDrivers
    env
    src
    gitcommit
    pythonWithPsycheExtension
    basePythonEnv
    ;

  mkWebsitePackage = pkgs.callPackage ../website/common.nix { };
}
