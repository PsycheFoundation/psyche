{
  buildPythonPackage,
  cmake,
  config,
  cudaPackages,
  einops,
  fetchFromGitHub,
  lib,
  ninja,
  psutil,
  setuptools,
  torch,
  wheel,
}:
let
  inherit (cudaPackages)
    cuda_cudart
    cuda_nvcc
    cuda_cccl
    flags
    ;
  inherit (lib.strings) concatStringsSep;

  cutlass = fetchFromGitHub {
    owner = "NVIDIA";
    repo = "cutlass";
    rev = "refs/tags/v4.0.0";
    sha256 = "sha256-HJY+Go1viPkSVZPEs/NyMtYJzas4mMLiIZF3kNX+WgA=";
  };
in
buildPythonPackage rec {
  __structuredAttrs = true;

  pname = "flash-attn";
  version = "2.8.2";

  src = fetchFromGitHub {
    owner = "Dao-AILab";
    repo = "flash-attention";
    rev = "refs/tags/v${version}";
    hash = "sha256-RrayQOxQzGJMQK5jmMziR59p8CTF8mpEyJsqzouEW1s=";
  };

  pyproject = true;

  postPatch = ''
    mkdir -p csrc/cutlass
    cp -r ${cutlass}/include csrc/cutlass/include
    substituteInPlace setup.py \
      --replace-fail \
        '+ cc_flag' \
        '+ ["${concatStringsSep ''","'' flags.gencode}"]' \
      --replace-fail \
        'compiler_c17_flag=["-O3", "-std=c++17"]' \
        'compiler_c17_flag=["-O3", "-std=c++17", "-mcmodel=medium"]'
  '';

  preConfigure = ''
    export BUILD_TARGET=cuda
    export FORCE_BUILD=TRUE
  '';

  enableParallelBuilding = true;

  build-system = [
    cmake
    ninja
    psutil
    setuptools
    wheel
  ];

  nativeBuildInputs = [
    cuda_nvcc
  ];

  dontUseCmakeConfigure = true;

  dependencies = [
    einops
    torch
  ];

  buildInputs = [
    cuda_cudart
    cuda_cccl
  ];

  doCheck = false;

  meta = {
    description = "Fast and memory-efficient exact attention";
    homepage = "https://github.com/Dao-AILab/flash-attention";
    license = lib.licenses.bsd3;
    platforms = lib.platforms.linux;
    maintainers = [ ];
    broken = !config.cudaSupport;
  };
}
