{
  inputs,
  system ? null,
  lib ? inputs.nixpkgs.lib,
}:

let
  cudaSupported = builtins.elem system [ "x86_64-linux" ];
  metalSupported = builtins.elem system [ "aarch64-darwin" ];
  gpuSupported = cudaSupported || metalSupported;
in
(
  lib.optionalAttrs (system != null) { inherit system; }
  // {
    overlays = lib.optionals cudaSupported [ inputs.nix-gl-host.overlays.default ] ++ [
      inputs.rust-overlay.overlays.default
      (final: prev: {
        python312Packages = prev.python312Packages.override {
          overrides = pyfinal: pyprev: rec {
            torch =
              if metalSupported then
                # Use PyTorch nightly for MPS to get uint support
                pyfinal.buildPythonPackage rec {
                  pname = "torch";
                  version = "2.9.0.dev20250731";
                  format = "wheel";

                  src = final.fetchurl {
                    url = "https://download.pytorch.org/whl/nightly/cpu/torch-${version}-cp312-none-macosx_11_0_arm64.whl";
                    hash = "sha256-0WADByPiZagUzUHYm6n5n30E+KZ78S63okLTYy9zNEs=";
                  };

                  propagatedBuildInputs = with pyfinal; [
                    filelock
                    typing-extensions
                    sympy
                    networkx
                    jinja2
                    numpy
                    requests
                    pyyaml
                    setuptools
                    fsspec
                  ];

                  doCheck = false;
                  pythonImportsCheck = [ "torch" ];

                  passthru = {
                    cudaSupport = false;
                  };
                }
              else
                pyprev.torch-bin;
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
      }
      // lib.optionalAttrs metalSupported {
        metalSupport = true;
      };
  }
)
