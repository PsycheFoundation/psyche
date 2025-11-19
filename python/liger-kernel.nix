{
  buildPythonPackage,
  fetchFromGitHub,
  torch,
  lib,
}:
buildPythonPackage {
  pname = "liger-kernel";
  version = "0.6.2-seed-oss";
  format = "setuptools";

  src = fetchFromGitHub {
    owner = "NousResearch";
    repo = "Liger-Kernel";
    rev = "4a35a4fd87afa631715014f1678cac4f153ef806";
    hash = "sha256-c3V9o4VR66V4O0lLOkavDWKoPzpHUXegvnDNjB7pwqQ=";
  };

  propagatedBuildInputs = [
    torch
  ];

  doCheck = false;

  meta = {
    description = "Efficient Triton kernels for LLM Training";
    homepage = "https://github.com/linkedin/Liger-Kernel";
    license = lib.licenses.bsd3;
    platforms = lib.platforms.linux;
  };
}
