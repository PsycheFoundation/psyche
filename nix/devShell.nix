{
  self,
  inputs,
  ...
}:
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
        buildWholeWorkspace
        env
        ;
    in
    {

      # fmt as precommit hook
      pre-commit = {
        check.enable = false;
        settings.hooks.treefmt.enable = true;
      };

      devShells.default = pkgs.mkShell {
        inputsFrom = [
          buildWholeWorkspace
          self'.packages.psyche-book
        ];
        inherit env;
        buildInputs = with pkgs; [
          # for local-testnet
          tmux
          nvtopPackages.full

          # task runner
          just

          # for some build scripts
          jq
          # it pretty :3
          nix-output-monitor

          # for running pkgs on non-nix
          pkgs.nix-gl-host

          # solana
          inputs'.solana-pkgs.packages.default

          # nixfmt
          nixfmt-rfc-style

          # for pnpm stuff
          nodejs
          pnpm
          wasm-pack
        ];

        shellHook = ''
          source ${lib.getExe config.agenix-shell.installationScript}
          ${config.pre-commit.installationScript}
          # put nixglhost paths in LD_LIBRARY_PATH so you can use gpu stuff on non-NixOS
          # the docs for nix-gl-host say this is a dangerous footgun but.. yolo
          export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:$(${pkgs.nix-gl-host}/bin/nixglhost -p)

          echo "Welcome to the Psyche development shell.";
        '';
      };
    };
}
