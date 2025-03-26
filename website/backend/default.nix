{ pkgs, ... }:
let
  psyche-website-wasm = pkgs.callPackage ../wasm { };
  psyche-website-shared = pkgs.callPackage ../shared { };
  mkWebsitePackage = pkgs.callPackage ../common.nix { };
in
mkWebsitePackage {
  package = "backend";
  meta.mainProgram = "backend";

  preBuild = ''
    mkdir -p wasm/dist
    cp -r ${psyche-website-wasm}/* wasm/pkg

    mkdir -p shared
    cp -r ${psyche-website-shared}/shared/* shared/

    export GITCOMMIT=${pkgs.psycheLib.gitcommit}
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/lib
    mkdir -p $out/bin

    cp -r backend/dist/* $out/lib/

    cat - <<EOF > $out/bin/backend
    #!/usr/bin/env bash
    exec ${pkgs.nodejs}/bin/node ${placeholder "out"}/lib/index.cjs "$@"
    EOF

    chmod +x $out/bin/backend

    runHook postInstall
  '';
}
