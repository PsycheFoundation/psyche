{
  stdenv,
  fetchurl,
  autoPatchelfHook,
  libpciaccess,
  libfabric,
  ucx,
  pmix,
  mpi,
  cudaPackages,
  lib,
}:
stdenv.mkDerivation rec {
  pname = "libnvshmem";
  version = "3.3.20";

  src = fetchurl {
    url = "https://developer.download.nvidia.com/compute/nvshmem/redist/libnvshmem/linux-x86_64/libnvshmem-linux-x86_64-${version}_cuda12-archive.tar.xz";
    hash = "sha256-dXRstC611auvROEo2SOKFiO2TNTUm8LE2O+cYI1Gx+E=";
  };

  nativeBuildInputs = [ autoPatchelfHook ];
  buildInputs = [
    stdenv.cc.cc.lib
    libpciaccess
    libfabric
    ucx
    pmix
    mpi
  ]
  ++ (with cudaPackages; [
    cuda_cudart
    cuda_nvcc
  ]);

  installPhase = ''
    runHook preInstall

    mkdir -p $out/{lib,include,bin,share}

    cp -r lib/* $out/lib/

    cp -r include/* $out/include/

    cp -r bin/* $out/bin/

    cp -r share/* $out/share/

    cp LICENSE $out/share/

    runHook postInstall
  '';

  postFixup = ''
    # Fix RPATH for binaries
    find $out/bin -type f -executable | while read -r file; do
      if [[ -f "$file" && ! -L "$file" ]]; then
        patchelf --set-rpath "${lib.makeLibraryPath buildInputs}:$out/lib" "$file" 2>/dev/null || true
      fi
    done

    # Fix RPATH for shared libraries
    find $out/lib -name "*.so*" | while read -r file; do
      if [[ -f "$file" && ! -L "$file" ]]; then
        patchelf --set-rpath "${lib.makeLibraryPath buildInputs}:$out/lib" "$file" 2>/dev/null || true
      fi
    done
  '';

  meta = with lib; {
    description = "NVIDIA SHMEM (NVSHMEM) is a parallel programming interface based on OpenSHMEM";
    homepage = "https://developer.nvidia.com/nvshmem";
    license = licenses.unfree;
    platforms = [ "x86_64-linux" ];
    maintainers = [ ];
  };
}
