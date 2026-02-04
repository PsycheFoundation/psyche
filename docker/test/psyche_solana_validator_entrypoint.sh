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

# Get authorizer program ID from keypair
AUTHORIZER_ID=$(solana address -k ./target/deploy/psyche_solana_authorizer-keypair.json)
echo "Authorizer program ID: ${AUTHORIZER_ID}"

anchor deploy --provider.cluster "${RPC}" --provider.wallet "/.config/solana/id.json" -- --max-len 500000
sleep 1

anchor idl init \
    --provider.cluster ${RPC} \
    --provider.wallet "/.config/solana/id.json" \
    --filepath ./target/idl/psyche_solana_authorizer.json \
    "${AUTHORIZER_ID}"
popd

pushd /local/solana-coordinator
echo -e "\n[+] Deploying Solana Coordinator"
anchor deploy --provider.cluster "${RPC}" -- --max-len 500000
popd

echo -e "\n[+] Verifying deployed programs:"

# Get program IDs from keypair files
AUTHORIZER_ID=$(solana address -k /local/solana-authorizer/target/deploy/psyche_solana_authorizer-keypair.json)
COORDINATOR_ID=$(solana address -k /local/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json)

echo "Checking Authorizer (${AUTHORIZER_ID}):"
solana account "${AUTHORIZER_ID}" --url "${RPC}" | grep -E "Executable|Owner" || echo "  NOT FOUND"

echo "Checking Coordinator (${COORDINATOR_ID}):"
solana account "${COORDINATOR_ID}" --url "${RPC}" | grep -E "Executable|Owner" || echo "  NOT FOUND"

# Check for optional programs (if they exist in the image)
if [ -f /local/solana-treasurer/target/deploy/psyche_solana_treasurer-keypair.json ]; then
    TREASURER_ID=$(solana address -k /local/solana-treasurer/target/deploy/psyche_solana_treasurer-keypair.json)
    echo "Checking Treasurer (${TREASURER_ID}):"
    solana account "${TREASURER_ID}" --url "${RPC}" | grep -E "Executable|Owner" || echo "  NOT FOUND (expected if not using treasurer features)"
fi

echo -e "\n[+] Validator ready, watching logs..."

# fg %1
solana logs --url "${RPC}" | grep -E "Pre-tick run state|Post-tick run state"
