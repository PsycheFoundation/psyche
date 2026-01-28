{
  lib,
  pnpmConfigHook,
  fetchPnpmDeps,
  pnpm,
  stdenv,
  nodejs,
  curl,
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
      extraNativeBuildInputs ? [ ],
      meta ? { },
    }@args:
    {
      __structuredAttrs = true;

      pname = "${packageJson.name}-${package}";
      version = packageJson.version;
      src = workspaceSrc;

      pnpmDeps = fetchPnpmDeps {
        inherit (finalAttrs) pname version;
        fetcherVersion = 2;
        src = workspaceSrc;
        hash = "sha256-PUXS9VkAOt9Gcjl0pdzHt0A3jmeSQFZ88+WFUqPgVxE=";
      };

      nativeBuildInputs = [
        pnpm
        pnpmConfigHook
        nodejs
        curl
      ]
      ++ extraNativeBuildInputs;

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
