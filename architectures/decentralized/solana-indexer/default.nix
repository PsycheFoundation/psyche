{
  lib,
  pnpm,
  stdenv,
  nodejs,
  psycheLib,
  ...
}:
let
  workspaceSrc = ./.;
  packageJson = lib.importJSON (workspaceSrc + "/package.json");
  package = "backend";
  pname = "${packageJson.name}-${package}";
  version = packageJson.version;
in
stdenv.mkDerivation {
  inherit pname version;
  src = workspaceSrc;

  pnpmDeps = pnpm.fetchDeps {
    inherit pname version;
    fetcherVersion = 2;
    src = workspaceSrc;
    hash = "";
  };

  nativeBuildInputs = [
    pnpm.configHook
    nodejs
  ];

  # pnpm stuff is a lilllll broken
  dontCheckForBrokenSymlinks = true;

  preBuild = ''
    export GITCOMMIT=${psycheLib.gitcommit}
  '';

  buildPhase = ''
    runHook preBuild

    pnpm build

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/lib
    mkdir -p $out/bin

    cp -r dist/* $out/lib/

    cat - <<EOF > $out/bin/backend
    #!/usr/bin/env bash
    exec ${nodejs}/bin/node ${placeholder "out"}/lib/index.cjs "$@"
    EOF

    chmod +x $out/bin/backend

    runHook postInstall
  '';

  checkPhase = "pnpm exec tsc -p . --noEmit";
  meta.mainProgram = "backend";
}
