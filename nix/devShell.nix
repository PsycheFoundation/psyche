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

                self'.packages.solana_toolbox_cli

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
              source ${lib.getExe config.agenix-shell.installationScript}
              ${config.pre-commit.installationScript}
              export UV_NO_MANAGED_PYTHON=1
              export UV_PYTHON=$(which python)
              export UV_CACHE_DIR=$PWD/.uv-cache
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
              export DYLD_LIBRARY_PATH="${pkgs.psycheLib.pythonSet.torch}/lib/python3.12/site-packages/torch/lib"
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
                psychePythonVenv
              ];
              shellHook = defaultShell.shellHook + ''
                echo "This shell has the 'psyche' module available in its python interpreter.";
              '';
            }
          );
        };
    };
}
