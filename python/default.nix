{
  pkgs,
}:
let

  inherit (pkgs.psycheLib)
    psycheLib
    cargoArtifacts
    craneLib
    rustWorkspaceArgs
    pytorch
    ;
  rustExtension = craneLib.buildPackage (
    rustWorkspaceArgs
    // {
      inherit cargoArtifacts;
      pname = "psyche-extension";

      nativeBuildInputs = rustWorkspaceArgs.nativeBuildInputs ++ [
        pkgs.maturin
        pkgs.python312
      ];

      buildPhaseCargoCommand = "maturin build --offline --target-dir ./target";

      installPhase = ''
        mkdir -p $out
        cp target/wheels/*.whl $out/
      '';

      doCheck = false;
    }
  );
in
pkgs.python312Packages.buildPythonPackage {
  pname = "psyche";
  version = "0.1.0";
  format = "wheel";

  src = "${rustExtension}/*.whl";

  propagatedBuildInputs = [
    pytorch
    pkgs.python312Packages.transformers
  ];

  doCheck = false;
}
