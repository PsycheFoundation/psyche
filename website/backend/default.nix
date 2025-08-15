{
  nodejs,
  psycheLib,
  psyche-website-wasm,
  psyche-website-shared,
}:
psycheLib.mkWebsitePackage {
  package = "backend";
  meta.mainProgram = "backend";

  preBuild = ''
    mkdir -p wasm/dist
    cp -r ${psyche-website-wasm}/* wasm/pkg

    mkdir -p shared
    cp -r ${psyche-website-shared}/shared/* shared/

    export GITCOMMIT=${psycheLib.gitcommit}
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/lib
    mkdir -p $out/bin

    cp -r backend/dist/* $out/lib/

    cat - <<EOF > $out/bin/backend
    #!/usr/bin/env bash
    exec ${nodejs}/bin/node max-old-space-size=12288 ${placeholder "out"}/lib/index.cjs "$@"
    EOF

    chmod +x $out/bin/backend

    runHook postInstall
  '';
}
