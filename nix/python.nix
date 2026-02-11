{
  python312,
  python312Packages,
  stdenvNoCC,
  lib,
  callPackage,
  uv2nix,
  pyproject-nix,
  pyproject-build-systems,
  extraPackages ? { }, # attrset of package names to derivations to include in the venv
  additionalNixPackages ? [ ], # list of additional nix-provided python package names (e.g., ["vllm"])
}:
let
  getAllTransitiveDeps =
    pkgNames:
    let
      getAllDeps =
        pkg:
        let
          direct = builtins.filter (d: d != null && d ? pname) (pkg.propagatedBuildInputs or [ ]);
          # only keep deps that exist in python312Packages
          inPkgSet = builtins.filter (d: d.pname != "python3" && python312Packages ? ${d.pname}) direct;
          indirect = lib.flatten (map getAllDeps inPkgSet);
        in
        lib.unique (inPkgSet ++ indirect);

      allDeps = lib.flatten (map (name: getAllDeps python312Packages.${name}) pkgNames);
      allDepNames = map (d: d.pname) allDeps;
    in
    lib.unique (pkgNames ++ allDepNames);

  # packages that we provide to the venv via nix derivations
  topLevelNixPkgs = [
    "torch"
  ]
  ++ additionalNixPackages
  ++ lib.optionals stdenvNoCC.hostPlatform.isLinux [
    "flash-attn"
    "liger-kernel"
    # i'm really not a fan of providing torchtitan like this. i'd much rather have it be built as a git dep via uv2nix.
    # i think there's room to figure out how to provide setuptools for it.
    "torchtitan"
  ];

  nixProvidedPythonPkgs = getAllTransitiveDeps topLevelNixPkgs;

  # uv2nix workspace for all deps from pyproject.toml / uv.lock
  workspace = uv2nix.lib.workspace.loadWorkspace { workspaceRoot = ../python; };

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
          pyproject-build-systems.overlays.default
          overlay
          (
            final: _prev:
            let
              hacks = callPackage pyproject-nix.build.hacks { };

              nixProvidedOverrides = builtins.listToAttrs (
                map (name: {
                  inherit name;
                  value = hacks.nixpkgsPrebuilt {
                    from = python312Packages.${name};
                  };
                }) nixProvidedPythonPkgs
              );
            in
            nixProvidedOverrides // extraPackages
          )
        ]
      );

  # Base venv packages (from uv.lock and nix-provided packages)
  baseVenvPackages = {
    psyche-deps = [ ];
  }
  // builtins.listToAttrs (
    map (name: {
      inherit name;
      value = [ ];
    }) nixProvidedPythonPkgs
  );

  # Add extra packages to venv
  venvPackages =
    baseVenvPackages
    // builtins.listToAttrs (
      map (name: {
        inherit name;
        value = [ ];
      }) (builtins.attrNames extraPackages)
    );

in
pythonSet.mkVirtualEnv "psyche-python-env" venvPackages
