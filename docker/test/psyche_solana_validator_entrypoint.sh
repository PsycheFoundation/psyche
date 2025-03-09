#!/bin/bash

set -o errexit
set -m

RPC=${RPC:-"http://localhost:8899"}

solana-test-validator -r &

sleep 3

pushd /usr/local/solana-coordinator
anchor deploy --provider.cluster "${RPC}" -- --max-len 500000
popd

# fg %1
solana logs --url "${RPC}" | grep -E "Pre-tick run state|Post-tick run state"
