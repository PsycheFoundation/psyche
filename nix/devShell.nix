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
            env = env // {
              UV_NO_SYNC = 1;
              # UV_PYTHON = pkgs.psycheLib.psychePythonVenv.interpreter;
              UV_PYTHON_DOWNLOADS = "never";
            };
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

              # Ensure LIBTORCH_USE_PYTORCH is set for runtime
              export LIBTORCH_USE_PYTORCH=1

              # Configure tmux to pass through libtorch environment variables
              tmux set-environment -g LIBTORCH_USE_PYTORCH 1 2>/dev/null || true
            ''
            + lib.optionalString pkgs.config.cudaSupport ''
              # put nixglhost paths in LD_LIBRARY_PATH so you can use gpu stuff on non-NixOS
              # the docs for nix-gl-host say this is a dangerous footgun but.. yolo
              export LD_LIBRARY_PATH=$(${pkgs.nix-gl-host}/bin/nixglhost -p):${pkgs.rdma-core}/lib
            ''
            + lib.optionalString pkgs.config.metalSupport (
              let
                torchLib = "${pkgs.python312Packages.torch}/lib/python3.12/site-packages/torch/lib";
                torchPath = "${pkgs.python312Packages.torch}/lib/python3.12/site-packages/torch";
                torchInclude = "${pkgs.python312Packages.torch}/lib/python3.12/site-packages/torch/include";
                pythonPath = "${psychePythonVenv}/${pkgs.python312.sitePackages}";
              in
              ''
                # macOS: Ensure PyTorch can use Metal Performance Shaders
                export PYTORCH_ENABLE_MPS_FALLBACK=1

                # Set up PyTorch paths for tch-rs runtime discovery
                export DYLD_LIBRARY_PATH="${torchLib}:''${DYLD_LIBRARY_PATH:-}"
                export DYLD_FALLBACK_LIBRARY_PATH="${torchLib}:''${DYLD_FALLBACK_LIBRARY_PATH:-}"
                export LIBTORCH="${torchPath}"
                export LIBTORCH_INCLUDE="${torchInclude}"
                export LIBTORCH_LIB="${torchLib}"

                # Set up Python path for embedded Python to find torch and dependencies
                export PYTHONPATH="${pythonPath}:''${PYTHONPATH:-}"

                # Set RUSTFLAGS to embed the rpath in Rust binaries at build time
                export RUSTFLAGS="-C link-args=-Wl,-rpath,${torchLib} ''${RUSTFLAGS:-}"

                # Configure tmux to pass through ALL environment variables for macOS
                # Use the same paths to ensure consistency
                tmux set-environment -g DYLD_LIBRARY_PATH "${torchLib}" 2>/dev/null || true
                tmux set-environment -g DYLD_FALLBACK_LIBRARY_PATH "${torchLib}" 2>/dev/null || true
                tmux set-environment -g LIBTORCH "${torchPath}" 2>/dev/null || true
                tmux set-environment -g LIBTORCH_INCLUDE "${torchInclude}" 2>/dev/null || true
                tmux set-environment -g LIBTORCH_LIB "${torchLib}" 2>/dev/null || true
                tmux set-environment -g PYTORCH_ENABLE_MPS_FALLBACK 1 2>/dev/null || true
                tmux set-environment -g PYTHONPATH "${pythonPath}" 2>/dev/null || true
                tmux set-environment -g RUSTFLAGS "-C link-args=-Wl,-rpath,${torchLib}" 2>/dev/null || true
              ''
            )
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
