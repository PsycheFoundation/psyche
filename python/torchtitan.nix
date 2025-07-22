{
  lib,
  buildPythonPackage,
  fetchFromGitHub,
  pythonOlder,
  setuptools,
  wheel,

  # deps
  torchdata,
  datasets,
  tomli,
  tyro,
  tensorboard,
  tabulate,
  fsspec,
  tokenizers,
  safetensors,

  blobfile,
  tiktoken,
}:

buildPythonPackage rec {
  pname = "torchtitan";
  version = "6e61dbc";
  pyproject = true;

  src = fetchFromGitHub {
    owner = "nousresearch";
    repo = "torchtitan";
    rev = "f6646555e44dea1d1ec269d5716919dfaf3a08b6";
    hash = "sha256-fzWZmwEIgDAXnJJwPnBb5Eyw18Lgx7Q2zcehAavfsro=";
  };

  build-system = [
    setuptools
    wheel
  ];

  dependencies = [
    torchdata
    datasets
    tensorboard
    tabulate
    fsspec
    tokenizers
    safetensors
    tyro
    tomli

    blobfile
    tiktoken
  ];

  doCheck = false;

  pythonImportsCheck = [
    "torchtitan"
  ];

  meta = with lib; {
    description = "A native PyTorch library for large model training";
    homepage = "https://github.com/NousResearch/torchtitan";
    license = licenses.bsd3;
  };
}
