{
  lib,
  buildPythonPackage,
  fetchFromGitHub,
  setuptools,
  pythonOlder,
  torch,
  requests,
  urllib3,
  # Optional dependencies
  fsspec ? null,
  iopath ? null,
  portalocker ? null,
  # Test dependencies
  pytest,
  pytest-cov,
  expecttest,
}:

buildPythonPackage rec {
  pname = "torchdata";
  version = "0.11.0";
  format = "setuptools";
  src = fetchFromGitHub {
    owner = "pytorch";
    repo = "data";
    rev = "v${version}";
    hash = "sha256-TSkZLL4WDSacuX4tl0+1bKSJCRI3LEhAyU3ztdlUvgk=";
  };

  disabled = pythonOlder "3.8";

  nativeBuildInputs = [
    setuptools
  ];

  propagatedBuildInputs = [
    torch
    requests
    urllib3
  ]
  ++ lib.optionals (fsspec != null) [
    fsspec
  ]
  ++ lib.optionals (iopath != null) [
    iopath
  ]
  ++ lib.optionals (portalocker != null) [
    portalocker
  ];

  nativeCheckInputs = [
    pytest
    pytest-cov
    expecttest
  ];

  pythonImportsCheck = [
    "torchdata"
  ];

  doCheck = false;

  checkPhase = ''
    runHook preCheck
    pytest test/
    runHook postCheck
  '';

  meta = with lib; {
    description = "A PyTorch repo for data loading and utilities to be used by pytorch/pytorch";
    homepage = "https://github.com/pytorch/data";
    platforms = platforms.unix;
  };
}
