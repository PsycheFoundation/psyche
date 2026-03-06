#! /bin/bash

set -o errexit
set -m

RPC=${RPC:-"http://localhost:8899"}

solana-keygen new --no-bip39-passphrase --force
solana config set --url localhost
solana-test-validator -r &

sleep 3

echo -e "\n[+] Deploying Solana Authorizer"

# Deploy authorizer program
solana program deploy /local/solana-authorizer/target/deploy/psyche_solana_authorizer.so \
    --program-id /local/solana-authorizer/target/deploy/psyche_solana_authorizer-keypair.json \
    --keypair /.config/solana/id.json \
    --url "${RPC}" \
    --max-len 500000

sleep 1

# IDL init (still needs anchor)
AUTHORIZER_ID=$(solana address -k /local/solana-authorizer/target/deploy/psyche_solana_authorizer-keypair.json)
echo "Authorizer program ID: ${AUTHORIZER_ID}"

anchor idl init \
    --provider.cluster ${RPC} \
    --provider.wallet "/.config/solana/id.json" \
    --filepath /local/solana-authorizer/target/idl/psyche_solana_authorizer.json \
    "${AUTHORIZER_ID}"

echo -e "\n[+] Deploying Solana Coordinator"

solana program deploy /local/solana-coordinator/target/deploy/psyche_solana_coordinator.so \
    --program-id /local/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json \
    --keypair /.config/solana/id.json \
    --url "${RPC}" \
    --max-len 500000

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
