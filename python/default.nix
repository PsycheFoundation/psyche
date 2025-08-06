{
  pkgs,
  system,
}:
let
  inherit (pkgs.psycheLib)
    psycheLib
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

      cargoExtraArgs = rustWorkspaceArgs.cargoExtraArgs + " --package psyche-python-extension";

      nativeBuildInputs = rustWorkspaceArgs.nativeBuildInputs ++ [
        pkgs.python312
      ];
      doCheck = false;
    }
  );

in
pkgs.python312Packages.buildPythonPackage rec {
  pname = "psyche";
  version = "0.1.0";
  format = "other"; # skip setup.py, we're assembling it ourselves

  src = ./python/psyche;

  propagatedBuildInputs = with pkgs.python312Packages; [
    torch-bin
    transformers
    (pkgs.python312Packages.callPackage ./flash-attn.nix { })
  ];

  installPhase = ''
    runHook preInstall

    # create python package dir
    mkdir -p $out/${pkgs.python312.sitePackages}/psyche

    # copy all python files
    cp -r * $out/${pkgs.python312.sitePackages}/psyche/

    # copy the extension .so file
    cp ${rustExtension}/lib/lib${builtins.replaceStrings [ "-" ] [ "_" ] rustExtension.pname}.so \
       $out/${pkgs.python312.sitePackages}/psyche/_psyche_ext.so

    runHook postInstall
  '';

  doCheck = false;
}
