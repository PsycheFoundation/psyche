{
  inputs,
  system ? null,
  lib ? inputs.nixpkgs.lib,
}:

let
  cudaSupported = builtins.elem system [ "x86_64-linux" ];
  metalSupported = builtins.elem system [ "aarch64-darwin" ];
  cudaVersion = "12.8";
in
(
  lib.optionalAttrs (system != null) { inherit system; }
  // {
    overlays = lib.optionals cudaSupported [ inputs.nix-gl-host.overlays.default ] ++ [
      inputs.rust-overlay.overlays.default
      (final: prev: {
        python312Packages = prev.python312Packages.override {
          overrides = pyfinal: pyprev: {
            # Add passthru attributes to torch so vLLM can detect CUDA support
            torch =
              (if pyprev ? torch-bin_2_9 then pyprev.torch-bin_2_9 else pyprev.torch-bin).overrideAttrs
                (old: {
                  passthru = (old.passthru or { }) // {
                    cudaSupport = cudaSupported;
                    rocmSupport = false;
                    cudaPackages = final.cudaPackages;
                    cudaCapabilities =
                      if cudaSupported then
                        final.cudaPackages.cudaFlags.cudaCapabilities or [
                          "8.9"
                          "9.0"
                          "10.0"
                        ]
                      else
                        [ ];
                    cxxdev = pyprev.torch.cxxdev or null;
                  };
                });
            flash-attn = pyfinal.callPackage ../python/flash-attn.nix { };
            liger-kernel = pyfinal.callPackage ../python/liger-kernel.nix { };
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

    config = {
      allowUnfree = true;
      metalSupport = lib.mkDefault false;
    }
    // lib.optionalAttrs cudaSupported {
      cudaSupport = true;
      inherit cudaVersion;
    }
    // lib.optionalAttrs metalSupported {
      metalSupport = true;
    };
  }
)
