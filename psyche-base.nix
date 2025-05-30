{ pkgs ? import <nixpkgs> {
    config = {
      allowUnfree = true;
      cudaSupport = true;
    };
  }
}:

# Step 1: Get, unzip and download libtorch
let
  libtorch = pkgs.stdenv.mkDerivation rec {
    name = "libtorch";
    lib_version = "2.6.0";
    cuda_version = "124";

    src = pkgs.fetchurl {
      url = "https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-${lib_version}%2Bcu${cuda_version}.zip";
      sha256 = "sha256-viGyrQ14SP7T+QlxGImlSYZLbPBtVkxYRU7rMqduqq4=";
    };

    nativeBuildInputs = [ pkgs.unzip ];

    unpackPhase = ''
      unzip $src
    '';

    installPhase = ''
      mkdir -p $out
      cp -r libtorch/* $out/

      rm -f $out/lib/libnvrtc*.so*
      rm -f $out/lib/libcuda*.so*
      rm -f $out/lib/libcufft*.so*
      rm -f $out/lib/libcurand*.so*
      rm -f $out/lib/libcusolver*.so*
      rm -f $out/lib/libcusparse*.so*
      rm -f $out/lib/libnvjpeg*.so*
      rm -f $out/lib/libnvToolsExt*.so*
    '';
  };

  # Build the psyche app with all dependencies and packages.
  psycheApp = pkgs.rustPlatform.buildRustPackage rec {
    pname = "psyche-solana-client";
    version = "0.1.0";

    # Filter out problematic problematic files and compiled.
    src = pkgs.lib.cleanSourceWith {
      src = ./.;
      filter = path: type:
        let
          pathStr = toString path;
        in
        !(pkgs.lib.hasInfix "/test-ledger" pathStr) &&
        !(pkgs.lib.hasInfix "/target" pathStr);
    };

    useFetchCargoVendor = true;
    cargoHash = "sha256-chYniz9he/SW8IXVkXitW/ZQY9gMH2gyM8Veu/zyh5Y=";

    # Equivalent to apt install in Dockerfile
    buildInputs = with pkgs; [
      openssl
      openssl.dev
      libgcc
      curl
      wget
      fontconfig
      fontconfig.dev
      pkg-config
      gcc
      gnumake
      binutils

      cudaPackages.cudatoolkit
      cudaPackages.cudnn
    ];

    # Equivalent to ENV in Dockerfile
    CUDA_HOME = "${pkgs.cudaPackages.cudatoolkit}";
    LIBTORCH = "${libtorch}";
    LIBTORCH_INCLUDE = "${libtorch}";
    LIBTORCH_LIB = "${libtorch}";
    LD_LIBRARY_PATH = "${pkgs.cudaPackages.cudatoolkit}/lib64:${libtorch}/lib";

    # Build psyche client.
    buildPhase = ''
      runHook preBuild

      # Set up environment variables
      export CUDA_HOME="${pkgs.cudaPackages.cudatoolkit}"
      export LIBTORCH="${libtorch}"
      export LIBTORCH_INCLUDE="${libtorch}"
      export LIBTORCH_LIB="${libtorch}"
      export LD_LIBRARY_PATH="${pkgs.cudaPackages.cudatoolkit}/lib64:${libtorch}/lib:$LD_LIBRARY_PATH"
      export PATH="${pkgs.cudaPackages.cudatoolkit}/bin:$PATH"
      export CPATH="${pkgs.cudaPackages.cudatoolkit}/include:$CPATH"

      # Build the binaries (equivalent to the RUN cargo build commands)
      cargo build -p psyche-solana-client --release --features parallelism
      cargo build -p psyche-centralized-client --release --features parallelism
      cargo build --example inference --release --features parallelism
      cargo build --example train --release --features parallelism

      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall

      mkdir -p $out/bin

      # Copy built binaries
      find target/release -maxdepth 1 -type f -executable -name "psyche-*" -exec cp {} $out/bin/ \;

      # Copy examples
      if [ -d target/release/examples ]; then
        find target/release/examples -maxdepth 1 -type f -executable \( -name "inference" -o -name "train" \) -exec cp {} $out/bin/ \;
      fi

      runHook postInstall
    '';

    doCheck = false;
  };

  # Create a runtime environment with all necessary libraries
  runtimeEnv = pkgs.buildEnv {
    name = "psyche-runtime";
    paths = with pkgs; [
      # Runtime libraries
      openssl
      libgcc
      curl
      fontconfig
      cudaPackages.cudatoolkit
      libtorch

      # Basic utilities
      coreutils
      bash
      glibc
    ];
  };

in

# Build the Docker image
pkgs.dockerTools.buildImage {
  name = "psyche-solana-client";
  tag = "latest";

  # Copy the built application and runtime environment
  copyToRoot = pkgs.buildEnv {
    name = "image-base";
    paths = [
      psycheApp
      runtimeEnv
      pkgs.dockerTools.usrBinEnv
      pkgs.dockerTools.binSh
    ];
    pathsToLink = [ "/bin" "/lib" "/lib64" "/share" ];
  };

  # Set up the container configuration
  config = {
    # Working directory (equivalent to WORKDIR)
    WorkingDir = "/usr/src";

    # Environment variables
    Env = [
      "CUDA_HOME=${pkgs.cudaPackages.cudatoolkit}"
      "LIBTORCH=${libtorch}"
      "LIBTORCH_INCLUDE=${libtorch}"
      "LIBTORCH_LIB=${libtorch}"
      "LD_LIBRARY_PATH=${pkgs.cudaPackages.cudatoolkit}/lib64:${libtorch}/lib"
      "PATH=${pkgs.cudaPackages.cudatoolkit}/bin:/bin:/usr/bin"
      "CPATH=${pkgs.cudaPackages.cudatoolkit}/include"
    ];

    # Default command - adjust based on your main binary
    Cmd = [ "/bin/psyche-solana-client" ];
  };
}
