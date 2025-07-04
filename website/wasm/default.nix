{
  stdenv,
  bash,
  psyche-deserialize-zerocopy-wasm,
}:
stdenv.mkDerivation {
  name = "psyche-website-wasm";

  src = [
    ./fixup.sh
  ];

  unpackPhase = ''
    for srcFile in $src; do
      cp $srcFile $(stripHash $srcFile)
    done
  '';

  buildPhase = ''
    runHook preBuild

    echo "copying pkg..."
    cp -r ${psyche-deserialize-zerocopy-wasm}/pkg .
    chmod 775 pkg -R

    echo "copying bindings..."
    cp -r ${psyche-deserialize-zerocopy-wasm}/bindings .
    chmod 775 bindings -R 

    mkdir $out
    cp -r pkg $out

    runHook postBuild
  '';

  postFixup = ''
    echo "running fixup..."
    ${bash}/bin/bash ./fixup.sh
  '';
}
