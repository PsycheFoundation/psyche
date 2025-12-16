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
            torch = pyfinal.torch-bin;
            flash-attn = pyfinal.callPackage ../python/flash-attn.nix { };
            liger-kernel = pyfinal.callPackage ../python/liger-kernel.nix { };
            torchtitan = pyfinal.callPackage ../python/torchtitan.nix { };
            torchdata = pyfinal.callPackage ../python/torchdata.nix { };
            tyro = pyprev.tyro.overridePythonAttrs (old: {
              propagatedBuildInputs = builtins.filter (
                dep:
                !builtins.elem (dep.pname or "") [
                  "jax"
                  "jaxlib"
                ]
              ) (old.propagatedBuildInputs or [ ]);
              doCheck = false;
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
