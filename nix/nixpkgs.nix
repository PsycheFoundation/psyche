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
        # temporary fix until https://github.com/NixOS/nixpkgs/pull/471394 is merged
        # that lets us use `torch` instead of `torch-bin`
        cudaPackages = prev.cudaPackages // {
          cudnn = prev.cudaPackages.cudnn.overrideAttrs (old: {
            appendRunpaths = (old.appendRunpaths or [ ]) ++ [
              "${prev.lib.getLib prev.cudaPackages.cuda_nvrtc}/lib"
            ];
          });
        };

        # provide packages for uv2pip to include
        python312Packages = prev.python312Packages.override {
          overrides = pyfinal: pyprev: {
            flash-attn = pyfinal.callPackage ../python/flash-attn.nix { };
            liger-kernel = pyfinal.callPackage ../python/liger-kernel.nix { };
            torchtitan = pyfinal.callPackage ../python/torchtitan.nix { };
            torchdata = pyfinal.callPackage ../python/torchdata.nix { };
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
