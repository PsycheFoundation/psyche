{
  perSystem =
    {
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
        psychePythonVenv
        psychePythonVenvWithExtension
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
              self'.packages.psyche-website-backend
            ];
            env = env // {
              UV_NO_SYNC = 1;
              # UV_PYTHON = pkgs.psycheLib.psychePythonVenv.interpreter;
              UV_PYTHON_DOWNLOADS = "never";
              NIX_LDFLAGS = "-L${psychePythonVenv}/lib -lpython3.12";
            };
            packages =
              with pkgs;
              [
                # to fix weird escapes
                bashInteractive

                # for local-testnet
                tmux
                nvtopPackages.full

                # task runner
                just

                # for some build scripts
                jq
                gnused # not installed by default on MacOS!

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
                cargo-nextest

                self'.packages.solana-toolbox-cli

                # for ci emulation
                inputs'.garnix-cli.packages.default

                # python stuff
                uv
              ]
              ++ (with inputs'.solana-pkgs.packages; [
                solana
                anchor
              ])
              ++ rustWorkspaceArgs.buildInputs
              ++ rustWorkspaceArgs.nativeBuildInputs;

            shellHook = ''
              export SHELL=${pkgs.bashInteractive}/bin/bash
              source ${lib.getExe config.agenix-shell.installationScript}
              ${config.pre-commit.installationScript}
            ''
            + lib.optionalString pkgs.config.cudaSupport ''
              # put nixglhost paths in LD_LIBRARY_PATH so you can use gpu stuff on non-NixOS
              # the docs for nix-gl-host say this is a dangerous footgun but.. yolo
              export LD_LIBRARY_PATH=$(${pkgs.nix-gl-host}/bin/nixglhost -p):${pkgs.rdma-core}/lib
            ''
            + lib.optionalString pkgs.config.metalSupport ''
              # macOS: Ensure PyTorch can use Metal Performance Shaders
              export PYTORCH_ENABLE_MPS_FALLBACK=1

              # Set up PyTorch library path for test execution
              export DYLD_LIBRARY_PATH="${psychePythonVenv}/lib/python3.12/site-packages/torch/lib"
            ''
            + ''
              echo "Welcome to the Psyche development shell.";
            '';
          };
          pythonShell = craneLib.devShell (
            defaultShell
            // {
              packages = defaultShell.packages ++ [
                psychePythonVenvWithExtension
              ];
              env = defaultShell.env // {
                # Override NIX_LDFLAGS to use the venv with extension
                NIX_LDFLAGS = "-L${psychePythonVenvWithExtension}/lib -lpython3.12";
              };
              shellHook = defaultShell.shellHook + ''
                echo "This shell has the 'psyche' module available in its python interpreter.";
              '';
            }
          );
        in
        {
          default = craneLib.devShell defaultShell;
          python = pythonShell;
          dev-python = pythonShell; # old name, kept for backwards compatibility
        };
    };
}
