{ inputs, ... }:
{
  imports = [ inputs.treefmt-nix.flakeModule ];

  perSystem =
    { pkgs, lib, ... }:
    {
      treefmt = {
        projectRootFile = "./flake.nix";
        programs = {
          just.enable = true;
          rustfmt = {
            enable = true;
            package = pkgs.psycheLib.rustToolchain;
          };
          taplo.enable = true; # toml files
          nixfmt.enable = true;

          clang-format = {
            enable = true;
            includes = [ "*.glsl" ];
          };

          prettier = {
            # js, ts, etc.
            enable = true;
            settings = {
              trailingComma = "es5";
              useTabs = true;
              semi = false;
              singleQuote = true;
            };
          };

          # python stuff
          black.enable = true;

          beautysh = {
            enable = true;
            indent_size = 4;
          };
        };
        settings.formatter.rustfmt.options =
          let
            rustfmtConfig = {
              skip_children = true;
              error_on_line_overflow = true;
              imports_granularity = "crate";
            };
          in
          [
            "--config"
            (lib.concatStringsSep "," (
              lib.mapAttrsToList (
                k: v: "${k}=${if builtins.isBool v then lib.boolToString v else v}"
              ) rustfmtConfig
            ))
          ];
        settings.global.excludes = [
          "**/*.svg"
          "**/*.gitignore"
          "**/*.go"
          "**/*.txt"
          "**/*.age"
          "**/.env*"
          "**/*.Dockerfile"
          "**/*.conf"
          "**/*.png"
          "**/*.jpg"
          "**/*.woff2"
          "**/*.pdf"
          "**/*.ds"
          "**/*.npy"
          "**/*.xml"
          "**/*.hbs"
          "**/*.min.js"
          ".envrc"
          ".dockerignore"
          ".gitattributes"
          "**/pnpm-lock.yaml"
        ];
      };
    };
}
