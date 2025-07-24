{
  lib,
  buildPythonPackage,
  fetchFromGitHub,
  setuptools,
  wheel,
  ninja,
  cmake,
  git,
  torch,
  urllib3,
  requests,
}:

buildPythonPackage rec {
  pname = "torchdata";
  version = "0.12.0a0";
  format = "setuptools";
  src = fetchFromGitHub {
    owner = "pytorch";
    repo = "data";
    rev = "a05a54f797dd0f1a66610652a949fd47243ff952";
    hash = "sha256-Wa4w1SOJ+sK7IuIq6zMwUKp6Hj6zp7NnkNtAqWbf/xw=";
  };

  # Build-time dependencies
  nativeBuildInputs = [
    setuptools
    wheel
    ninja
    # cmake
    git
  ];

  # Runtime dependencies
  propagatedBuildInputs = [
    torch
    urllib3
    requests
  ];

  # Set environment variables that the build expects
  preBuild = ''
    # Ensure git is available for version detection
    export PATH="${git}/bin:$PATH"

    # Set BUILD_VERSION to avoid git dependency during build if needed
    # export BUILD_VERSION="${version}"
  '';

  # The build process expects these files to exist
  prePatch = ''
    # Ensure version.txt exists if building from source
    if [ ! -f version.txt ]; then
      echo "${version}" > version.txt
    fi

    # Ensure requirements.txt exists with correct content
    cat > requirements.txt << EOF
    urllib3 >= 1.25
    requests
    EOF
  '';

  # Since get_ext_modules() returns [], there are no C++ extensions to build
  # But we still need the build dependencies available
  buildInputs = [
    # cmake
    ninja
  ];

  doCheck = false;
  pythonImportsCheck = [
    "torchdata"
  ];

  meta = with lib; {
    description = "Composable data loading modules for PyTorch";
    homepage = "https://github.com/pytorch/data";
    license = licenses.bsd3;
    maintainers = with maintainers; [ ]; # Add your maintainer info
    platforms = platforms.unix;
  };
}
