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
          overrides =
            pyfinal: pyprev:
            {
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
            }
            // lib.optionalAttrs cudaSupported {

              flashinfer = pyfinal.buildPythonPackage {
                pname = "flashinfer";
                version = "0.3.1";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };

              # Create stub packages for all nvidia-* CUDA dependencies (they're in torch wheels)
              nvidia-cuda-runtime-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cuda-runtime-cu12";
                version = "12.8.90";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cuda-nvrtc-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cuda-nvrtc-cu12";
                version = "12.8.93";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-nccl-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-nccl-cu12";
                version = "2.27.5";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cudnn-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cudnn-cu12";
                version = "9.10.2.21";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cublas-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cublas-cu12";
                version = "12.8.4.1";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-nvjitlink-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-nvjitlink-cu12";
                version = "12.8.93";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cufft-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cufft-cu12";
                version = "11.3.3.83";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cusparse-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cusparse-cu12";
                version = "12.5.8.93";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-nvtx-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-nvtx-cu12";
                version = "12.8.90";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cuda-cupti-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cuda-cupti-cu12";
                version = "12.8.90";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-curand-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-curand-cu12";
                version = "10.3.9.90";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cusolver-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cusolver-cu12";
                version = "11.7.3.90";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cufile-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cufile-cu12";
                version = "1.13.1.3";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-cusparselt-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-cusparselt-cu12";
                version = "0.7.1";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              nvidia-nvshmem-cu12 = pyfinal.buildPythonPackage {
                pname = "nvidia-nvshmem-cu12";
                version = "3.3.20";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };

              vllm =
                (pyprev.vllm.override {
                  cudaSupport = true;
                  cudaPackages = final.cudaPackages;
                  rocmSupport = false;
                }).overridePythonAttrs
                  (old: {
                    # Remove torchvision - fails to build CUDA extensions
                    propagatedBuildInputs = lib.filter (dep: (dep.pname or "") != "torchvision") (
                      old.propagatedBuildInputs or [ ]
                    );
                    # Completely disable dependency checking
                    pythonImportsCheck = [ ];
                    dontCheckRuntimeDeps = true;

                    # Skip egg_info that checks dependencies
                    preBuild = ''
                      export SKIP_DEP_CHECK=1
                    '';
                  });

              bitsandbytes = pyprev.bitsandbytes.overridePythonAttrs (old: {
                nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ final.ninja ];
                buildInputs = (old.buildInputs or [ ]) ++ [
                  final.cudaPackages.cuda_cudart
                  final.cudaPackages.cuda_cccl
                  final.cudaPackages.libcublas
                  final.cudaPackages.libcusparse
                ];
              });

              # depyf needs openssl binary in PATH for torch inductor
              depyf = pyprev.depyf.overridePythonAttrs (old: {
                nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ final.openssl ];
                doCheck = false; # Skip tests that require openssl
              });

              # llguidance also needs openssl binary for torch inductor tests
              llguidance = pyprev.llguidance.overridePythonAttrs (old: {
                nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ final.openssl ];
                doCheck = false; # Skip tests that require openssl
              });

              # xgrammar has triton version conflict with torch - remove triton dep (torch provides it)
              xgrammar = pyprev.xgrammar.overridePythonAttrs (old: {
                dontUsePythonCatchConflicts = true;
              });

              # jax tests fail - skip them
              jax = pyprev.jax.overridePythonAttrs (old: {
                doCheck = false;
              });

              # torchvision CUDA compilation fails - replace with dummy package
              torchvision = pyfinal.buildPythonPackage {
                pname = "torchvision";
                version = "0.24.0";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
              # torchvision CUDA compilation fails - replace with dummy package
              torchaudio = pyfinal.buildPythonPackage {
                pname = "torchaudio";
                version = "2.9.0";
                format = "other";
                dontUnpack = true;
                installPhase = "mkdir -p $out";
              };
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
      allowBroken = true;
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
