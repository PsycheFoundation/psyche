{
  psycheLib,
  python312,
  stdenvNoCC,
  config,
  lib,
}:
let
  inherit (psycheLib)
    cargoArtifacts
    craneLib
    rustWorkspaceArgs
    ;

  # build the actual rust extension that the python psyche code loads
  rustExtension = craneLib.buildPackage (
    rustWorkspaceArgs
    // {
      inherit cargoArtifacts;
      pname = "psyche-python-extension";
      cargoExtraArgs =
        " --package psyche-python-extension"
        + lib.optionalString (config.cudaSupport) " --features parallelism";
      doCheck = false;
    }
  );

  # expected lib file ext for the python extension
  ext = if stdenvNoCC.isDarwin then "dylib" else "so";

in
# a combination of the python files and rust ext for the psyche python code
stdenvNoCC.mkDerivation {
  __structuredAttrs = true;

  name = "psyche";
  version = "0.1.0";

  src = ./python/psyche;

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
