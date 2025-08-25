{
  inputs,
  system ? null,
  lib ? inputs.nixpkgs.lib,
}:

let
  cudaSupported = builtins.elem system [ "x86_64-linux" ];
  cudaVersion = "12.8";
in
(
  lib.optionalAttrs (system != null) { inherit system; }
  // {
    overlays = lib.optionals cudaSupported [ inputs.nix-gl-host.overlays.default ] ++ [
      inputs.rust-overlay.overlays.default
      (final: prev: {
        cudaPackages = prev.cudaPackages // {
          libnvshmem = final.callPackage ./nvshmem.nix { };
        };
        python312Packages = prev.python312Packages.override {
          overrides = pyfinal: pyprev: {
            setuptools = pyfinal.callPackage ./torch/setuptools.nix { inherit (pyprev.setuptools) patches; };
            torch = pyfinal.callPackage ./torch { inherit (pyprev.torch) patches; };
            torch-bin = throw "torch-bin not supported.";
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
    }
    // lib.optionalAttrs cudaSupported {
      cudaSupport = true;
      inherit cudaVersion;
    };
  }
)
