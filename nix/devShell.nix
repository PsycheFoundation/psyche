{
  perSystem =
    {
      system,
      config,
      pkgs,
      lib,
      inputs',
      self',
      ...
    }:
    let
      inherit (pkgs.psycheLib)
        rustWorkspaceArgs
        craneLib
        env
        pythonWithPsycheExtension
        ;
    in
    {
      # fmt as precommit hook
      pre-commit = {
        check.enable = false;
        settings.hooks.treefmt.enable = true;
      };

      devShells =
        let
          defaultShell = {
            inputsFrom = [
              self'.packages.psyche-book
            ];
            inherit env;
            packages =
              with pkgs;
              [
                # for local-testnet
                tmux
                nvtopPackages.full

                # task runner
                just

                # for some build scripts
                jq

                # it pretty :3
                nix-output-monitor

                # treefmt
                self'.formatter

                # for pnpm stuff
                nodejs
                pnpm
                wasm-pack

                # cargo stuff
                cargo-watch
              ]
              ++ (with inputs'.solana-pkgs.packages; [
                solana
                anchor
              ])
              ++ rustWorkspaceArgs.buildInputs
              ++ rustWorkspaceArgs.nativeBuildInputs;

            shellHook =
              ''
                source ${lib.getExe config.agenix-shell.installationScript}
                ${config.pre-commit.installationScript}
              ''
              + lib.optionalString pkgs.config.cudaSupport ''
                # put nixglhost paths in LD_LIBRARY_PATH so you can use gpu stuff on non-NixOS
                # the docs for nix-gl-host say this is a dangerous footgun but.. yolo
                export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:$(${pkgs.nix-gl-host}/bin/nixglhost -p)
              ''
              + ''
                echo "Welcome to the Psyche development shell.";
              '';
          };
        in
        {
          default = craneLib.devShell defaultShell;
          dev-python = craneLib.devShell (
            defaultShell
            // {
              packages = defaultShell.packages ++ [
                pythonWithPsycheExtension
              ];
              shellHook =
                defaultShell.shellHook
                + ''
                  echo "This shell has the 'psyche' module available in its python interpreter.";
                '';
            }
          );
          dev-torchtitan-python =
            let
              originalPsyche = pkgs.callPackage ../python { };
              torchtitan = pkgs.python312Packages.callPackage ../python/torchtitan.nix {
                torchdata = (pkgs.python312Packages.callPackage ../python/torchdata.nix { });
              };

              # the psyche python package, but skip the torchtitan import
              psycheWithRuntimeTorchtitan = pkgs.python312Packages.buildPythonPackage rec {
                inherit (originalPsyche)
                  pname
                  version
                  src
                  installPhase
                  doCheck
                  ;
                format = "other";

                propagatedBuildInputs =
                  # filter out torchtitan from original deps
                  (builtins.filter (pkg: (pkg.pname or "") != "torchtitan") originalPsyche.propagatedBuildInputs)
                  # but keep torchtitan's dependencies
                  ++ torchtitan.dependencies;
              };

              pythonWithPsycheExtensionRuntimeTorchtitan = pkgs.python312.withPackages (ps: [
                psycheWithRuntimeTorchtitan
              ]);

            in
            craneLib.devShell (
              defaultShell
              // {
                packages =
                  defaultShell.packages
                  ++ (with pkgs.python312Packages; [
                    pythonWithPsycheExtensionRuntimeTorchtitan
                    virtualenv
                    pip
                  ]);

                shellHook =
                  defaultShell.shellHook
                  + ''
                    echo "This shell has a 'psyche' python module that expects torchtitan from the runtime environment.";

                    VENV_DIR=".nix-torchtitan-venv"

                    if [ ! -d "$VENV_DIR" ]; then
                      echo "creating venv..."
                      python -m venv "$VENV_DIR"
                    fi

                    echo "activating venv..."
                    source "$VENV_DIR/bin/activate"

                    if python -c "import torchtitan" 2>/dev/null; then
                        echo "torchtitan found in environment at location"
                        TORCHTITAN_LOCATION=$(pip show torchtitan | grep Location | cut -d' ' -f2)
                        echo "Location: $TORCHTITAN_LOCATION"
                        
                        # Get the absolute path of the venv directory
                        VENV_ABS_PATH=$(realpath "$VENV_DIR")
                        
                        # Check if the torchtitan location contains the venv directory path
                        if [[ "$TORCHTITAN_LOCATION" == *"$VENV_ABS_PATH"* ]]; then
                            echo "✓ torchtitan is installed in the virtual environment"
                            echo "\`import psyche\` should now work in python and use your local torchtitan copy."
                        else
                            echo ""
                            echo "⚠️  WARNING: TORCHTITAN NOT PROVIDED BY THIS VENV! ⚠️"
                            echo "=================================================="
                            echo "Expected location to contain: $VENV_ABS_PATH"
                            echo "Actual location:              $TORCHTITAN_LOCATION"
                            echo ""
                            echo "This means torchtitan is installed globally or in a different environment!"
                            echo "To fix this, first uninstall torchtitan globally, then run: \`pip install -e .\` in your torchtitan folder, while in this venv."
                            echo "=================================================="
                            echo ""
                        fi
                    else
                        echo "✗ torchtitan not found! Install it with \`pip install -e .\` in your torchtitan folder, while in this venv."
                    fi

                    echo "To deactivate the venv, exit this nix shell via ctrl-c / ctrl-d / exit."
                  '';
              }
            );
        };
    };
}
