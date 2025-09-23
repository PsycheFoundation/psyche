{
  buildPythonPackage,
  fetchFromGitHub,
  torch,
  lib,
}:
buildPythonPackage rec {
  pname = "liger-kernel";
  version = "0.6.2";
  format = "setuptools";

  src = fetchFromGitHub {
    owner = "linkedin";
    repo = "Liger-Kernel";
    rev = "v${version}";
    hash = "sha256-Ys3P8V6qkIucOaROsevRgnGwq0NJAJsDs6dupgPMudQ=";
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
    maintainers = with lib.maintainers; [ ];
  };
}
