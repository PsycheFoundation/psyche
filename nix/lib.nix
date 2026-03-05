{
  pkgs,
  inputs,
  lib ? pkgs.lib,
  gitcommit ? inputs.self.rev or inputs.self.dirtyRev or "unknown",
}:
let
  optionalApply = cond: f: if cond then f else lib.id;
  util = import ./util.nix;
  system = pkgs.stdenv.hostPlatform.system;

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
    || (builtins.match ".*shared/client/src/state/prompt_texts/index\\.json$" path != null)
    || (builtins.match ".*shared/client/src/parallelism_data\\.json$" path != null);

  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type: (testResourcesFilter path type) || (craneLib.filterCargoSources path type);
  };

  env = {
    LIBTORCH_USE_PYTORCH = 1;
  };

  rustWorkspaceDeps = {
    nativeBuildInputs = with pkgs; [
      python312
      pkg-config
      perl
      cargo-nextest
    ];

    buildInputs =
      (with pkgs; [
        openssl
        python312Packages.torch
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
      psychePythonVenvWithExtension
    ];
    NIX_LDFLAGS = "-L${psychePythonVenvWithExtension}/lib -lpython3.12";
  };

  rustWorkspaceArgsNoPython = rustWorkspaceDeps // {
    inherit env src;
    strictDeps = true;
    # Enable parallelism feature only on CUDA-supported platforms
    cargoExtraArgs = lib.optionalString (pkgs.config.cudaSupport) "--features parallelism";
  };

  cargoArtifacts = craneLib.buildDepsOnly rustWorkspaceArgs;
  cargoArtifactsNoPython = craneLib.buildDepsOnly rustWorkspaceArgsNoPython;

  psychePythonExtension = pkgs.callPackage ../python { };

  # python venv without the psyche extension (vllm, etc)

  pythonDeps = { inherit (inputs) uv2nix pyproject-nix pyproject-build-systems; };
  psychePythonVenv = pkgs.callPackage ./python.nix (
    {
      extraPackages = { };
    }
    // pythonDeps
  );

  # python venv with the psyche extension
  psychePythonVenvWithExtension = pkgs.callPackage ./python.nix (
    {
      extraPackages = {
        psyche = psychePythonExtension;
      };
    }
    // pythonDeps
  );
  # builds a rust package
  # Returns an attrset of packages: { packageName = ...; packageName-nopython = ...; }
  # Automatically discovers and builds examples from the crate's examples/ directory
  # Automatically discovers and builds integration tests from the crate's tests/ directory
  # Auto-detects if package has a main binary by checking for src/main.rs or src/bin/
  # needsPython: true = only with Python + ext, false = only without Python + ext, "optional" = both variants
  # needsGpu: wraps the package with nix-gl-host
  # supportedSystems: list of systems to build on (e.g., [ "x86_64-linux" "aarch64-linux" ]), null means all systems
  # buildInputs: attrset of runtime dependencies for each built binary
  #   - buildInputs.main = [ deps ] applies to src/main.rs
  #   - buildInputs.<type> = [ deps ] applies to all binaries of type (bin/test/example)
  #   - buildInputs.<type>.<name> = [ deps ] applies to specific binary
  buildRustPackage =
    {
      needsPython ? false,
      needsGpu ? false,
      cratePath, # path to the crate dir
      supportedSystems ? null,
      buildInputs ? { },
    }:
    let
      # type: "main" | "bin" | "test" | "example"
      # name: the binary / test name
      getRuntimeDepsForArtifact =
        type: name:
        let
          typeConfig = buildInputs.${type} or null;

          # there's only one "main"
          mainDeps =
            if type == "main" then
              (
                if lib.isList typeConfig then
                  typeConfig
                else if typeConfig != null then
                  throw "buildInputs.main must be a list, got ${builtins.typeOf typeConfig}"
                else
                  [ ]
              )
            else
              [ ];

          # for other types, take both list (all bins) and attrset (specific bin)
          typeDeps = if type != "main" then (if lib.isList typeConfig then typeConfig else [ ]) else [ ];

          specificDeps =
            if type != "main" && lib.isAttrs typeConfig then (typeConfig.${name} or [ ]) else [ ];
        in
        mainDeps ++ typeDeps ++ specificDeps;

      buildMaybePythonRustPackage =
        {
          name,
          type,
          withPython,
          originalName ? name,
          dir ? null,
        }:
        assert lib.assertMsg (builtins.elem type [
          "bin"
          "test"
          "example"
        ]) "type must be 'bin', 'test', or 'example', got: ${type}";
        let
          workspaceArgs = if withPython then rustWorkspaceArgsWithPython else rustWorkspaceArgsNoPython;
          artifacts = if withPython then cargoArtifacts else cargoArtifactsNoPython;

          # delete conflicting bins from other crates to prevent ambiguous --bin/--example/--test
          deleteConflicts =
            let
              # main binaries use packageName, others use originalName
              binaryName = if type == "bin" && dir == null then packageName else originalName;
              # main binaries (dir==null) delete from src/bin, others use their dir
              conflictingDir = if dir == null then "src/bin" else dir;
            in
            ''
              echo "Resolving workspace binary conflicts for ${type} ${originalName} from crate ${packageName}"

              # get workspace root and this crate's manifest path from cargo metadata
              crate_manifest=$(cargo metadata --format-version 1 --no-deps | \
                jq -r '.packages[] | select(.name == "${packageName}") | .manifest_path')
              workspace_root=$(cargo metadata --format-version 1 --no-deps | jq -r '.workspace_root')
              crate_dir=$(dirname "$crate_manifest")
              crate_relative_path=$(realpath --relative-to="$workspace_root" "$crate_dir")

              ${lib.optionalString (type == "bin" && dir == null) ''
                # validate no conflicting src/bin/${packageName}.rs in same crate
                if [ -f "$crate_relative_path/src/main.rs" ] && [ -f "$crate_relative_path/${conflictingDir}/${binaryName}.rs" ]; then
                  echo "Error: crate ${packageName} has both src/main.rs and src/bin/${packageName}.rs"
                  echo "This makes '--bin ${packageName}' ambiguous."
                  exit 1
                fi
              ''}

              # delete conflicting files from all workspace members
              while IFS= read -r member_id; do
                member_manifest=$(cargo metadata --format-version 1 --no-deps | \
                  jq -r ".packages[] | select(.id == \"$member_id\") | .manifest_path")
                member_name=$(cargo metadata --format-version 1 --no-deps | \
                  jq -r ".packages[] | select(.id == \"$member_id\") | .name")
                member_dir=$(dirname "$member_manifest")
                member_relative=$(realpath --relative-to="$workspace_root" "$member_dir")

                # skip our own crate
                if [ "$member_relative" != "$crate_relative_path" ]; then
                  conflict_file="$member_relative/${conflictingDir}/${binaryName}.rs"
                  if [ -f "$conflict_file" ]; then
                    echo "Deleting conflicting file: $conflict_file"
                    rm "$conflict_file"
                  fi
                fi

                ${lib.optionalString (type == "bin" && dir == "src/bin") ''
                  # delete src/main.rs if package name matches binary name
                  if [ "$member_name" = "${originalName}" ]; then
                    conflict_file="$member_relative/src/main.rs"
                    if [ -f "$conflict_file" ]; then
                      echo "Deleting conflicting file: $conflict_file (package name conflicts with binary name)"
                      rm "$conflict_file"
                    fi
                  fi
                ''}
              done < <(cargo metadata --format-version 1 --no-deps | jq -r '.workspace_members[]')
            '';

          rustPackage = craneLib.buildPackage (
            workspaceArgs
            // {
              cargoArtifacts = artifacts;
              pname = name;
              cargoExtraArgs = workspaceArgs.cargoExtraArgs + " --${type} ${originalName}";
              doCheck = false;
              meta.mainProgram = name;

              # delete conflicting files from other crates to avoid ambiguity in what to build
              preBuild = deleteConflicts;
              nativeBuildInputs = workspaceArgs.nativeBuildInputs ++ [ pkgs.jq ];

              doInstallCargoArtifacts = false;
              installPhase = ''
                runHook preInstall
                mkdir -p $out/bin

                ${
                  if type == "test" then
                    # tests have hash suffixes and live in deps/
                    ''
                      expected_binary_dir="target/release/deps"
                      built_binary=$(find "$expected_binary_dir" -maxdepth 1 -name "${originalName}-*" -type f -executable | head -n1)
                    ''
                  else
                    # binaries are in release/ with exact name, examples are in target/release/examples with exact name
                    ''
                      expected_binary_dir="target/release${lib.optionalString (type == "example") "/examples"}"
                      built_binary="$expected_binary_dir/${originalName}"
                    ''
                }

                if [ -n "$built_binary" ] && [ -f "$built_binary" ]; then
                  cp "$built_binary" $out/bin/${name}
                  chmod +x $out/bin/${name}
                else
                  echo "Error: binary ${originalName} not found in $expected_binary_dir"
                  echo "Contents of $expected_binary_dir:"
                  ls -la "$expected_binary_dir/" || true
                  exit 1
                fi

                runHook postInstall
              '';
            }
          );

          runtimeDeps = getRuntimeDepsForArtifact type originalName;
          allRuntimeDeps = (lib.optionals withPython [ psychePythonVenvWithExtension ]) ++ runtimeDeps;

          wrappedRustPackage =
            pkgs.runCommand "${name}"
              {
                buildInputs = [ pkgs.makeWrapper ];
                meta.mainProgram = name;
              }
              ''
                mkdir -p $out/bin
                makeWrapper ${rustPackage}/bin/${name} $out/bin/${name} \
                  ${lib.concatMapStringsSep " " (dep: "--prefix PATH : \"${dep}/bin\"") allRuntimeDeps}
              '';
        in
        if allRuntimeDeps != [ ] then wrappedRustPackage else rustPackage;

      # build a target with python/nopython variants
      buildTarget =
        {
          name,
          originalName ? name,
          type,
          needsPython,
          needsGpu,
          dir ? null,
        }:
        let
          maybeWrapGpu = optionalApply needsGpu useHostGpuDrivers;

          mkVariant =
            withPython:
            maybeWrapGpu (buildMaybePythonRustPackage {
              inherit
                type
                dir
                name
                originalName
                withPython
                ;
            });

          withPython = mkVariant true;
          withoutPython = mkVariant false;

        in
        if needsPython == "optional" then
          {
            ${name} = withPython;
            "${name}-nopython" = withoutPython;
          }
        else if lib.isBool needsPython then
          { ${name} = if needsPython then withPython else withoutPython; }
        else
          throw "needsPython must be true, false, or \"optional\", got: ${builtins.toString needsPython}";

      allRsFilenamesInDir =
        dir:
        let
          entries = lib.optionalAttrs (builtins.pathExists dir) (builtins.readDir dir);
          rustFiles = lib.filterAttrs (n: v: v == "regular" && lib.hasSuffix ".rs" n) entries;
        in
        lib.mapAttrsToList (name: _: lib.removeSuffix ".rs" name) rustFiles;

      buildTargetsFromDir =
        {
          dir,
          type,
          needsPython,
          needsGpu,
          prefix ? "",
        }:
        let
          absoluteDir = cratePath + "/${dir}";
          targetNames = allRsFilenamesInDir absoluteDir;
          buildOne =
            name:
            buildTarget {
              inherit
                type
                needsPython
                needsGpu
                dir
                ;
              originalName = name;
              name = "${prefix}${name}";
            };
        in
        builtins.foldl' (acc: name: acc // (buildOne name)) { } targetNames;

      cargoToml = builtins.fromTOML (builtins.readFile (cratePath + "/Cargo.toml"));
      packageName = cargoToml.package.name;
      hasMainRs = builtins.pathExists (cratePath + "/src/main.rs");

      # build src/main.rs if it exists (output is guaranteed unique by crate name)
      mainRsPackage = lib.optionalAttrs hasMainRs (buildTarget {
        name = packageName;
        type = "bin";
        inherit needsPython needsGpu;
      });

      binDirPackages = buildTargetsFromDir {
        dir = "src/bin";
        type = "bin";
        prefix = "bin-${packageName}-";
        inherit needsPython needsGpu;
      };

      examplePackages = buildTargetsFromDir {
        dir = "examples";
        type = "example";
        inherit needsPython needsGpu;
      };

      testPackages = buildTargetsFromDir {
        dir = "tests";
        type = "test";
        prefix = "test-${packageName}-";
        inherit needsPython needsGpu;
      };

      shouldBuildForThisSystem = supportedSystems == null || builtins.elem system supportedSystems;

      validateBuildInputs =
        let
          binNames = allRsFilenamesInDir (cratePath + "/src/bin");
          exampleNames = allRsFilenamesInDir (cratePath + "/examples");
          testNames = allRsFilenamesInDir (cratePath + "/tests");

          availableArtifacts = {
            main = lib.optional hasMainRs packageName;
            bin = binNames;
            example = exampleNames;
            test = testNames;
          };

          checkType =
            type:
            let
              typeConfig = buildInputs.${type} or null;
              available = availableArtifacts.${type} or [ ];
            in
            if typeConfig == null then
              null
            else if type == "main" then
              # main must exist
              if !hasMainRs then
                throw "buildInputs.main specified but ${packageName} has no src/main.rs"
              else
                null
            else if lib.isList typeConfig then
              null # list format is always valid for a type
            else if lib.isAttrs typeConfig then
              let
                specifiedNames = lib.attrNames typeConfig;
                invalidNames = lib.filter (name: !(lib.elem name available)) specifiedNames;
              in
              if invalidNames != [ ] then
                throw "buildInputs.${type} specifies non-existent artifacts: ${lib.concatStringsSep ", " invalidNames}. Available ${type} artifacts: ${lib.concatStringsSep ", " available}"
              else
                null
            else
              throw "buildInputs.${type} must be a list or attrset, got ${builtins.typeOf typeConfig}";

          invalidTypes = lib.filter (
            t:
            !(lib.elem t [
              "main"
              "bin"
              "example"
              "test"
            ])
          ) (lib.attrNames buildInputs);
          typeChecks = lib.map checkType (lib.attrNames buildInputs);
        in
        x:
        if invalidTypes != [ ] then
          throw "buildInputs has invalid types: ${lib.concatStringsSep ", " invalidTypes}. Valid types: main, bin, example, test"
        else
          assert (builtins.all (c: c == null) typeChecks);
          x;
    in
    validateBuildInputs (
      lib.optionalAttrs shouldBuildForThisSystem (
        util.mergeAttrsetsNoConflicts "can't merge binary package sets" [
          mainRsPackage
          binDirPackages
          examplePackages
          testPackages
        ]
      )
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
            binaryen # wasm-opt
            wasm-pack
            jq
            wasm-bindgen-cli
          ]);

        buildPhaseCargoCommand = ''
          export CRATE_PATH=$(cargo metadata --format-version=1 --no-deps | jq -r ".packages[] | select(.name == \"${name}\") | .manifest_path" | xargs dirname)

          echo "building wasm"
          # wasm-pack needs a $HOME dir set.
          RUST_LOG=debug HOME=$TMPDIR wasm-pack build --target nodejs --mode no-install $CRATE_PATH

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

  useHostGpuDrivers = optionalApply pkgs.config.cudaSupport (
    package:
    assert lib.assertMsg (
      package.meta ? mainProgram
    ) "Package ${package.name} must have meta.mainProgram set to use useHostGpuDrivers";
    pkgs.runCommand "${package.name}-nixgl-wrapped"
      {
        nativeBuildInputs = [ pkgs.makeWrapper ];
        meta.mainProgram = package.meta.mainProgram;
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
  );

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
    buildRustPackage
    buildRustWasmTsPackage
    useHostGpuDrivers
    env
    src
    gitcommit
    psychePythonVenv
    psychePythonVenvWithExtension
    ;

  mkWebsitePackage = pkgs.callPackage ../website/common.nix { };
}
