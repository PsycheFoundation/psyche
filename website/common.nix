{ pkgs, ... }:
let
  workspaceSrc = ./.;
  packageJson = pkgs.lib.importJSON (workspaceSrc + "/package.json");
  pnpmDeps = pkgs.pnpm.fetchDeps {
    pname = packageJson.name;
    version = packageJson.version;
    src = workspaceSrc;
    hash = "sha256-FjZt0cNKlMBdgocLTbr6RkGMBjqu3rp7NWgyAX3imY4=";
  };
in
{
  package,
  preBuild,
  buildCommand ? "build",
  installPhase,
  extraInputs ? [ ],
  meta ? { },
}:
pkgs.stdenv.mkDerivation {
  pname = "${packageJson.name}-${package}";
  version = packageJson.version;
  src = workspaceSrc;

  inherit pnpmDeps meta;

  nativeBuildInputs =
    with pkgs;
    [
      pnpm.configHook
      nodejs
    ]
    ++ extraInputs;

  inherit preBuild installPhase;

  # pnpm stuff is a lilllll broken
  dontCheckForBrokenSymlinks = true;

  buildPhase = ''
    runHook preBuild

    pnpm -C ${package} exec tsc -p . --noEmit

    pnpm -C ${package} ${buildCommand}

    runHook postBuild
  '';

  checkPhase = "pnpm exec tsc -p . --noEmit";
}
