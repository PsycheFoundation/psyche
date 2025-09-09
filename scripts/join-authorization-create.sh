#!/bin/bash

set -o errexit
set -e
set -m

_usage() {
    echo "Usage: $0 <SOLANA_RPC> <GRANTOR_KEYPAIR_FILE> <GRANTEE_PUBKEY>"
    echo "  SOLANA_RPC: The solana RPC url or moniker to use"
    echo "  GRANTOR_KEYPAIR_FILE: The keypair file of the authority that is issuing the authorization"
    echo "  GRANTEE_PUBKEY: The pubkey that is the receiver of the authorization"
    exit 1
}

if [[ "$#" -lt 3 ]]; then
    _usage
fi

SOLANA_RPC="$1"
shift

GRANTOR_KEYPAIR_FILE="$1"
shift

if [[ ! -f "$GRANTOR_KEYPAIR_FILE" ]]; then
    echo "Error: Grantor keypair file '$GRANTOR_KEYPAIR_FILE' not found."
    _usage
fi
GRANTOR_PUBKEY=$(solana-keygen pubkey $GRANTOR_KEYPAIR_FILE)

GRANTEE_PUBKEY="$1"
shift

PSYCHE_AUTHORIZER_ID="PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw"
PSYCHE_AUTH_SCOPE="utf8:CoordinatorJoinRun"

# Make sure all is good to go
echo "SOLANA_RPC: $SOLANA_RPC"
echo "GRANTOR_KEYPAIR_FILE: $GRANTOR_KEYPAIR_FILE"
echo "GRANTOR_PUBKEY: $GRANTOR_PUBKEY"
echo "GRANTEE_PUBKEY: $GRANTEE_PUBKEY"
echo "PSYCHE_AUTHORIZER_ID: $PSYCHE_AUTHORIZER_ID"
echo "PSYCHE_AUTH_SCOPE: $PSYCHE_AUTH_SCOPE"

# Create a new authorization and save the created PDA's address
echo "----"
echo "Creating a new authorization..."
AUTHORIZATION_CREATE_JSON=$( \
        solana-toolbox --rpc=$SOLANA_RPC instruction \
        $PSYCHE_AUTHORIZER_ID authorization_create \
        payer:keypair \
        grantor:$GRANTOR_KEYPAIR_FILE \
        --args=params.grantee:$GRANTEE_PUBKEY \
        --args=params.scope:$PSYCHE_AUTH_SCOPE \
        --execute
)
echo $AUTHORIZATION_CREATE_JSON | jq -r .outcome.explorer_url
echo "----"

# Extract the authorization PDA from the JSON response
AUTHORIZATION_PUBKEY=$(echo $AUTHORIZATION_CREATE_JSON | jq -r .resolved.addresses.authorization)
echo "AUTHORIZATION_PUBKEY: $AUTHORIZATION_PUBKEY"

# Activate the new authorization we just created
echo "----"
echo "Activation of the newly created authorization..."
AUTHORIZATION_ACTIVATE_JSON=$( \
        solana-toolbox --rpc=$SOLANA_RPC instruction \
        $PSYCHE_AUTHORIZER_ID authorization_grantor_update \
        --args=params.active:true \
        grantor:$GRANTOR_KEYPAIR_FILE \
        authorization:$AUTHORIZATION_PUBKEY \
        --execute
)
echo $AUTHORIZATION_ACTIVATE_JSON | jq -r .outcome.explorer_url
echo "----"
