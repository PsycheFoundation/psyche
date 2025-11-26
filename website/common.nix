{
  lib,
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
      extraInputs ? [ ],
      meta ? { },
    }@args:
    {
      __structuredAttrs = true;

      pname = "${packageJson.name}-${package}";
      version = packageJson.version;
      src = workspaceSrc;

      pnpmDeps = pnpm.fetchDeps {
        inherit (finalAttrs) pname version;
        fetcherVersion = 2;
        src = workspaceSrc;
        hash = "sha256-PUXS9VkAOt9Gcjl0pdzHt0A3jmeSQFZ88+WFUqPgVxE=";
      };

      nativeBuildInputs = [
        pnpm.configHook
        nodejs
        curl
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
