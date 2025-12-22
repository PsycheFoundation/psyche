{
  lib,
  buildPythonPackage,
  fetchFromGitHub,
  setuptools,
  pythonOlder,
  # Core dependencies
  torchdata,
  datasets,
  tokenizers,
  tomli,
  fsspec,
  tyro,
  tensorboard,
  # Optional dependencies
  pre-commit,
  pytest,
  pytest-cov,
  wandb,
  tomli-w,
  expecttest,
  # Optional nanosets
  datatrove ? null,
  numba ? null,
  # Optional transformers
  transformers ? null,
  # Feature flags
  withDev ? false,
  withNanosets ? false,
  withTransformers ? false,
}:

let
  src = fetchFromGitHub {
    owner = "NousResearch";
    repo = "torchtitan";
    rev = "6720c3e9266f8dd9d1bb67f4397542b89056a382";
    hash = "sha256-LaqZuIhVeDWS6XUFf/mktAGFuSd1i7LLC2CIuz26/kY=";
  };
  version = lib.removeSuffix "\n" (builtins.readFile (src + "/assets/version.txt"));
in
buildPythonPackage {
  pname = "torchtitan";
  inherit src version;
  format = "pyproject";

  disabled = pythonOlder "3.10";

  nativeBuildInputs = [
    setuptools
  ];

  propagatedBuildInputs = [
    torchdata
    datasets
    tokenizers
    tomli
    fsspec
    tyro
    tensorboard
  ]
  ++ lib.optionals withDev [
    pre-commit
    pytest
    pytest-cov
    wandb
    tomli-w
    expecttest
  ]
  ++ lib.optionals withNanosets [
    datatrove
    numba
  ]
  ++ lib.optionals withTransformers [
    transformers
  ];

  nativeCheckInputs = [
    pytest
    pytest-cov
  ]
  ++ lib.optionals (!withDev) [
    tomli-w
    expecttest
  ];

  pythonImportsCheck = [
    "torchtitan"
  ];

  checkPhase = ''
    runHook preCheck
    pytest tests/
    runHook postCheck
  '';

  # Skip tests by default since they may require GPU
  doCheck = false;

  meta = with lib; {
    description = "A PyTorch native platform for training generative AI models";
    homepage = "https://github.com/NousResearch/torchtitan";
    platforms = platforms.unix;
  };
}
