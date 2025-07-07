{
  psyche-deserialize-zerocopy-wasm,
  runCommand,
}:
runCommand "psyche-website-wasm" { } ''
  echo "copying pkg..."
  cp -r ${psyche-deserialize-zerocopy-wasm}/pkg .
  chmod 775 pkg -R

  echo "copying bindings..."
  cp -r ${psyche-deserialize-zerocopy-wasm}/bindings .
  chmod 775 bindings -R 

  echo "fixing up exports"
  bash ${./fixup.sh}

  mkdir $out
  cp -r pkg $out
''
