{
  lib,
  pnpm,
  stdenv,
  nodejs,
  ...
}:
let
  workspaceSrc = ./.;
  packageJson = lib.importJSON (workspaceSrc + "/package.json");
in
lib.extendMkDerivation {
  constructDrv = stdenv.mkDerivation;

  extendDrvArgs =
    finalAttrs:
    {
      package,
      preBuild,
      buildCommand ? "build",
      installPhase,
      extraInputs ? [ ],
      meta ? { },
    }@args:
    {
      pname = "${packageJson.name}-${package}";
      version = packageJson.version;
      src = workspaceSrc;

      pnpmDeps = pnpm.fetchDeps {
        inherit (finalAttrs) pname version;
        src = workspaceSrc;
        hash = "sha256-FjZt0cNKlMBdgocLTbr6RkGMBjqu3rp7NWgyAX3imY4=";
      };

      nativeBuildInputs = [
        pnpm.configHook
        nodejs
      ]
      ++ extraInputs;

      inherit preBuild installPhase;

      # pnpm stuff is a lilllll broken
      dontCheckForBrokenSymlinks = true;

      buildPhase =
        args.buildPhase or ''
          runHook preBuild

          pnpm -C ${package} exec tsc -p . --noEmit

          pnpm -C ${package} ${buildCommand}

          runHook postBuild
        '';

      checkPhase = args.checkPhase or "pnpm exec tsc -p . --noEmit";

      inherit meta;
    };
}
