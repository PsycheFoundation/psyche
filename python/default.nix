{
  psycheLib,
  python312Packages,
  config,
  python312,
  system,
  lib,
  stdenv,
}:
let
  inherit (psycheLib)
    cargoArtifacts
    craneLib
    rustWorkspaceArgs
    ;

  # build the extension .so file using Crane
  rustExtension = craneLib.buildPackage (
    rustWorkspaceArgs
    // rec {
      inherit cargoArtifacts;
      pname = "psyche-python-extension";

      cargoExtraArgs =
        " --package psyche-python-extension"
        + lib.optionalString (config.cudaSupport) " --features parallelism";

      nativeBuildInputs = rustWorkspaceArgs.nativeBuildInputs ++ [
        python312
      ];
      doCheck = false;
    }
  );

  # expected lib file ext for the python extension
  ext = if stdenv.isDarwin then "dylib" else "so";

  # parse pyproject.toml to get expected dep versions
  pyprojectToml = builtins.fromTOML (builtins.readFile ./pyproject.toml);
  expectedVersions = lib.listToAttrs (
    map (
      dep:
      let
        parts = lib.splitString "==" dep;
        name = lib.head parts;
        version = if lib.length parts > 1 then lib.elemAt parts 1 else null;
      in
      {
        name = name;
        value = version;
      }
    ) pyprojectToml.project.dependencies
  );

  # verify versions match nixpkgs
  versionAssertions = lib.mapAttrsToList (
    depName: expectedVersion:
    let
      nixpkgsVersion = python312Packages.${depName}.version or null;
    in
    lib.assertMsg (expectedVersion == null || nixpkgsVersion == expectedVersion)
      "Version mismatch for ${depName}: expected ${toString expectedVersion} from pyproject.toml, got ${toString nixpkgsVersion} in nixpkgs."
  ) expectedVersions;
in
assert lib.all (x: x) versionAssertions;
python312Packages.buildPythonPackage rec {
  pname = "psyche";
  version = "0.1.0";
  format = "other"; # skip setup.py, we're assembling it ourselves

  src = ./python/psyche;

  propagatedBuildInputs = with python312Packages; [
    torch
    transformers
  ];
  # ++ (lib.optionals config.cudaSupport [
  #   (python312Packages.callPackage ./flash-attn.nix { })
  # ]);

  installPhase = ''
    runHook preInstall

    # create python package dir
    mkdir -p $out/${python312.sitePackages}/psyche

    # copy all python files
    cp -r * $out/${python312.sitePackages}/psyche/

    # copy the extension binary file
    cp ${rustExtension}/lib/lib${builtins.replaceStrings [ "-" ] [ "_" ] rustExtension.pname}.${ext} \
       $out/${python312.sitePackages}/psyche/_psyche_ext.so

    runHook postInstall
  '';

  doCheck = false;
}
