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

        clang-format = {
          enable = true;
          includes = [ "*.glsl" ];
        };

        prettier = {
          # js, ts, etc.
          enable = true;
          settings = {
            "arrowParens" = "always";
            "bracketSpacing" = true;
            "bracketSameLine" = false;
            "printWidth" = 80;
            "proseWrap" = "always";
            "semi" = true;
            "singleQuote" = false;
            "tabWidth" = 2;
            "trailingComma" = "all";
            "useTabs" = false;
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
