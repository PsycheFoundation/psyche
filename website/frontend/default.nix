{
  miningPoolRpc,
  miningPoolCluster,
  coordinatorCluster,
  backendPath,
  psyche-website-wasm,
  psyche-website-shared,
  psycheLib,
}:
psycheLib.mkWebsitePackage {
  package = "frontend";

  preBuild = ''
    mkdir -p wasm/dist
    cp -r ${psyche-website-wasm}/* wasm/pkg

    mkdir -p shared
    cp -r ${psyche-website-shared}/shared/* shared/

    cp ${../../shared/data-provider/tests/resources/llama2_tokenizer.json} frontend/public/tokenizers/
    cp ${../../shared/data-provider/tests/resources/llama3_tokenizer.json} frontend/public/tokenizers/

    cp ${../../shared/client/src/state/prompt_texts/index.json} frontend/public/prompts/
    export VITE_MINING_POOL_RPC=${miningPoolRpc}
    export VITE_BACKEND_PATH=${backendPath}
    export VITE_MINING_POOL_CLUSTER=${miningPoolCluster}
    export VITE_COORDINATOR_CLUSTER=${coordinatorCluster}
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out
    cp -r frontend/dist/* $out/

    runHook postInstall
  '';
}
