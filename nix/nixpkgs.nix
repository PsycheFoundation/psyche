{
  inputs,
  system ? null,
  lib ? inputs.nixpkgs.lib,
}:

let
  cudaSupported = builtins.elem system [ "x86_64-linux" ];
in
(
  lib.optionalAttrs (system != null) { inherit system; }
  // {
    overlays =
      lib.optionals cudaSupported [
        inputs.nix-gl-host.overlays.default
      ]
      ++ [
        inputs.rust-overlay.overlays.default
        (final: prev: {
          # nccl will be updated in nixpkgs soon. remove this after https://github.com/NixOS/nixpkgs/pull/427804 is merged
          cudaPackages = prev.cudaPackages // {
            nccl = prev.cudaPackages.nccl.overrideAttrs (oldAttrs: rec {
              version = "2.27.6-1";
              src = prev.fetchFromGitHub {
                owner = "NVIDIA";
                repo = "nccl";
                rev = "v${version}";
                hash = "sha256-/BiLSZaBbVIqOfd8nQlgUJub0YR3SR4B93x2vZpkeiU=";
              };
              postPatch = ''
                patchShebangs ./src/device/generate.py
                patchShebangs ./src/device/symmetric/generate.py
              '';
            });
          };
          python312Packages = prev.python312Packages.override {
            overrides = pyfinal: pyprev: rec {
              torch-bin =
                let
                  cudaVersion = "128";
                  version = "2.8.0";
                  srcs = {
                    "x86_64-linux-312" = prev.fetchurl (
                      if prev.config.cudaSupport then
                        {
                          url = "https://download.pytorch.org/whl/test/cu${cudaVersion}/torch-${version}%2Bcu${cudaVersion}-cp312-cp312-manylinux_2_28_x86_64.whl";
                          hash = "sha256-Q1T8Bbt5sgjWmVoEyhzu9qlUexxDNENVdDU9OBxVCHw=";
                        }
                      else
                        {
                          url = "https://download.pytorch.org/whl/test/cpu/torch-${version}%2Bcpu-cp312-cp312-manylinux_2_28_x86_64.whl";
                          hash = "sha256-y5qLqBN6sk42vxdCy3mhKUvTdNtXDwn8FaXhMYFg204=";
                        }
                    );
                    "aarch64-darwin-312" = prev.fetchurl {
                      url = "https://download.pytorch.org/whl/test/cpu/torch-${version}-cp312-none-macosx_11_0_arm64.whl";
                      hash = "sha256-pHt5hr7j9hrSF9ioziRgWAmrQluvNJ+X3nWIFe3S71Q=";
                    };
                  };
                  pyVerNoDot = builtins.replaceStrings [ "." ] [ "" ] pyfinal.python.pythonVersion;
                  unsupported = throw "Unsupported system";
                in
                pyprev.torch-bin.overrideAttrs (oldAttrs: rec {
                  inherit version;
                  src = srcs."${prev.stdenv.system}-${pyVerNoDot}" or unsupported;
                });

              torch = torch-bin;
              tyro = pyprev.tyro.overridePythonAttrs (oldAttrs: {
                doCheck = false;
                nativeCheckInputs = [ ];
              });
            };
          };
        })
        (
          final: prev:
          import ./pkgs.nix {
            pkgs = prev;
            inherit inputs;
          }
        )
      ];

    config =
      {
        allowUnfree = true;
      }
      // lib.optionalAttrs cudaSupported {
        cudaSupport = true;
        cudaVersion = "12.8";
      };
  }
)
