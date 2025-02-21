#!/bin/bash

set -o errexit
set -m

RPC=${RPC:-"http://localhost:8899"}

solana-keygen new --no-bip39-passphrase --force
solana config set --url localhost
solana-test-validator -r &

sleep 3

pushd /usr/local/solana-coordinator
anchor deploy --provider.cluster "${RPC}" -- --max-len 500000
popd

fg %1
