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
            torch = pyprev.torch-bin;
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
