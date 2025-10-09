{
  nodejs,
  psycheLib,
}:
psycheLib.mkWebsitePackage {
  package = "backend";
  meta.mainProgram = "backend";

  preBuild = ''
    export GITCOMMIT=${psycheLib.gitcommit}
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/lib
    mkdir -p $out/bin

    cp -r backend/dist/* $out/lib/

    cat - <<EOF > $out/bin/backend
    #!/usr/bin/env bash
    exec ${nodejs}/bin/node ${placeholder "out"}/lib/index.cjs "$@"
    EOF

    chmod +x $out/bin/backend

    runHook postInstall
  '';
}
