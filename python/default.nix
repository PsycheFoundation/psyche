{
  psycheLib,
  python312,
  stdenvNoCC,
  config,
  lib,
  callPackage,
  uv2nix,
  pyproject-nix,
  pyproject-build-systems,
}:
let
  # packages that we provide to the venv via nix derivations
  nixProvidedPythonPkgs = [
    "torch"
    "liger-kernel"
    "flash-attn"
  ];

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

  # a combination of the python files and rust ext for the pyche python code
  psyche-python-module = stdenvNoCC.mkDerivation {
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
  };

  # uv2nix workspace for all deps from pyproject.toml / uv.lock
  workspace = uv2nix.lib.workspace.loadWorkspace { workspaceRoot = ./.; };

  # idk lol hehe
  overlay = workspace.mkPyprojectOverlay {
    sourcePreference = "wheel";
  };

  # a set of python packages that we can create a venv out of
  pythonSet =
    (callPackage pyproject-nix.build.packages {
      python = python312;
    }).overrideScope
      (
        lib.composeManyExtensions [
          pyproject-build-systems.overlays.wheel
          overlay
          (
            final: _prev:
            let
              inherit (final) pkgs;
              hacks = pkgs.callPackage pyproject-nix.build.hacks { };

              makePrebuilt = name: {
                inherit name;
                value = hacks.nixpkgsPrebuilt {
                  from = pkgs.python312Packages.${name};
                };
              };
            in
            (builtins.listToAttrs (map makePrebuilt nixProvidedPythonPkgs))
            // {
              psyche = psyche-python-module;
            }
          )
        ]
      );

in
pythonSet.mkVirtualEnv "psyche-runtime-env" (
  {
    psyche-deps = [ ];
    psyche = [ ];
  }
  // builtins.listToAttrs (
    map (name: {
      inherit name;
      value = [ ];
    }) nixProvidedPythonPkgs
  )
)
