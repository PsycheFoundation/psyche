{
  pkgs,
  inputs,
  rustWorkspaceDeps,
  craneLib,
  src,
  lib ? pkgs.lib,
}:
let
  system = pkgs.stdenv.hostPlatform.system;

  solanaCraneLib =
    (inputs.crane.mkLib pkgs).overrideToolchain
      inputs.solana-pkgs.packages.${system}.solana-rust;

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

  buildSolanaProgram =
    {
      programName,
      workspaceDir,
      sourceRoot,
      keypair ? "",
    }:
    let
      cargoLock = workspaceDir + "/Cargo.lock";

      env = {
        # Note: do NOT use `-A warnings` here — cargo relies on the
        # "dropping unsupported crate type" warning from rustc to detect
        # SBF target capabilities. Suppressing it causes the build to fail.
        # See https://github.com/rust-lang/rust/issues/116626
        RUSTFLAGS = "--cfg procmacro2_semver_exempt";
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
          pname = "solana-program-${programName}";
          buildPhaseCargoCommand = "cargo test --no-run --features idl-build";
        }
      );

      cargoTomlContents = lib.importTOML (workspaceDir + "/programs/${programName}/Cargo.toml");
      binaryName = cargoTomlContents.package.name;
      # Anchor uses underscored names for .so files
      soName = builtins.replaceStrings [ "-" ] [ "_" ] binaryName;

      solana = inputs.solana-pkgs.packages.${system}.solana;
      anchor = inputs.solana-pkgs.packages.${system}.anchor;
    in
    # Use mkCargoDerivation from solanaCraneLib for source handling and deps
    # unpacking, but anchor build uses its own cargo-build-sbf from the solana
    # package (which correctly sets SBF_SDK_PATH and RUSTC).
    solanaCraneLib.mkCargoDerivation (
      solanaWorkspaceArgs
      // {
        cargoArtifacts = solanaCargoArtifacts;
        pname = programName;
        version = "0";
        pnameSuffix = "-program";

        postPatch = ''
          if [ -n "${keypair}" ]; then
            mkdir -p ./target/deploy
            cp ${keypair} ./target/deploy/${soName}-keypair.json
          fi
        '';

        nativeBuildInputs = [
          anchor
          solana
        ]
        ++ rustWorkspaceDeps.nativeBuildInputs;

        buildPhaseCargoCommand = ''
          anchor build
        '';

        installPhase = ''
          runHook preInstall
          mkdir -p $out
          cp target/deploy/${soName}.so $out/
          cp target/deploy/${soName}-keypair.json $out/
          # Also build and include the IDL
          anchor idl build --out $out/${soName}.json
          runHook postInstall
        '';

        doInstallCargoArtifacts = false;
      }
    );
in
{
  inherit
    solanaCraneLib
    buildSolanaIdl
    buildSolanaProgram
    ;
}
