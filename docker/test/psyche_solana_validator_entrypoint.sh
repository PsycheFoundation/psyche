#! /bin/bash

set -o errexit
set -m

RPC=${RPC:-"http://localhost:8899"}

solana-keygen new --no-bip39-passphrase --force
solana config set --url localhost
solana-test-validator -r &

sleep 3

pushd /local/solana-authorizer
echo -e "\n[+] Creating authorization for everyone to join the run"
anchor deploy --provider.cluster "${RPC}" --provider.wallet "/.config/solana/id.json" -- --max-len 500000
sleep 1

anchor idl init \
    --provider.cluster ${RPC} \
    --provider.wallet "/.config/solana/id.json" \
    --filepath /local/solana-authorizer/target/idl/psyche_solana_authorizer.json \
    PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw
popd

pushd /local/solana-coordinator
anchor deploy --provider.cluster "${RPC}" -- --max-len 500000
popd

# fg %1
solana logs --url "${RPC}" | grep -E "Pre-tick run state|Post-tick run state"
