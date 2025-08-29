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
        python312Packages = prev.python312Packages.override {
          overrides = pyfinal: pyprev: rec {
            torch-bin =
              let
                libnvshmem = final.callPackage ./nvshmem.nix { };
                # 12.8 -> 128, etc.
                pyCudaVer = builtins.replaceStrings [ "." ] [ "" ] cudaVersion;
                version = "2.9.0.dev20250811";
                nightly = true;
                srcs = {
                  "x86_64-linux-312" = prev.fetchurl {
                    url = "https://download.pytorch.org/whl/${
                      if nightly then "nightly/" else ""
                    }cu${pyCudaVer}/torch-${version}%2Bcu${pyCudaVer}-cp312-cp312-manylinux_2_28_x86_64.whl";
                    hash = "sha256-N4y1ClwOFYz20p4SoLBuuB/zwqoAfeO4n8Ds9FZFpg0=";
                  };
                  "aarch64-darwin-312" = prev.fetchurl {
                    url = "https://download.pytorch.org/whl/${
                      if nightly then "nightly/" else ""
                    }cpu/torch-${version}-cp312-none-macosx_11_0_arm64.whl";
                    hash = "sha256-2KaV5j9AZdBk7Voxtu0Mf4K03g9AHBpZk8MWGwbV8Nw=";
                  };
                };
                pyVerNoDot = builtins.replaceStrings [ "." ] [ "" ] pyfinal.python.pythonVersion;
                unsupported = sys: throw "No pytorch wheel URL configured for ${sys}";
              in
              pyprev.torch-bin.overrideAttrs (oldAttrs: {
                inherit version;
                src =
                  srcs."${prev.stdenv.system}-${pyVerNoDot}" or (unsupported "${prev.stdenv.system}-${pyVerNoDot}");

                buildInputs =
                  oldAttrs.buildInputs
                  ++ lib.optionals final.stdenv.hostPlatform.isLinux [
                    libnvshmem
                  ];
              });

            torch = torch-bin;
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
