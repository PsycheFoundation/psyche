{ inputs, ... }:
{
  imports = [ inputs.treefmt-nix.flakeModule ];

  perSystem = _: {
    treefmt = {
      projectRootFile = "./flake.nix";
      programs = {
        just.enable = true;
        rustfmt.enable = true;
        taplo.enable = true; # toml files
        nixfmt.enable = true;
        mdformat = {
          enable = true;
          settings.number = true;
        };

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
