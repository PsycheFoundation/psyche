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
          overrides = pyfinal: pyprev: rec {
            torch-bin =
              let
                libnvshmem = final.callPackage ./nvshmem.nix { };
                # 12.8 -> 128, etc.
                pyCudaVer = builtins.replaceStrings [ "." ] [ "" ] cudaVersion;
                version = "2.9.0.dev20250827";
                nightly = true;
                srcs = {
                  "x86_64-linux-312" = prev.fetchurl {
                    url = "https://download.pytorch.org/whl/${
                      if nightly then "nightly/" else ""
                    }cu${pyCudaVer}/torch-${version}%2Bcu${pyCudaVer}-cp312-cp312-manylinux_2_28_x86_64.whl";
                    hash = "sha256-q8cQYRFQjef0vY7gaJZLGcIAMOmcCyx9BzMMVwKujdc=";
                  };
                  "aarch64-darwin-312" = prev.fetchurl {
                    url = "https://download.pytorch.org/whl/${
                      if nightly then "nightly/" else ""
                    }cpu/torch-${version}-cp312-none-macosx_11_0_arm64.whl";
                    hash = "sha256-7Kaa4oH6vDDlktQ/WijN20MdYsJOzsEsANHggzzqIBU=";
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
            transformers = pyprev.transformers.overrideAttrs (oldAttrs: {
              version = "4.56.1";
              src = prev.fetchFromGitHub {
                owner = "huggingface";
                repo = "transformers";
                tag = "v${oldAttrs.version}";
                hash = "sha256-92l1eEiqd3R9TVwNDBee6HsyfnRW1ezEi5fzVqmh76c=";
              };
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
      disableFlashAttn = lib.mkDefault (builtins.getEnv "DISABLE_FLASH_ATTN" == "1");
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
