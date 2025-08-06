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

            shellHook = ''
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
              shellHook = defaultShell.shellHook + ''
                echo "This shell has the 'psyche' module available in its python interpreter.";
              '';
            }
          );
        };
    };
}
