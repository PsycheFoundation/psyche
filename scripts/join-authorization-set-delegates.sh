#!/bin/bash

set -o errexit
set -e
set -m

_usage() {
    echo "Usage: $0 <SOLANA_RPC> <GRANTOR_PUBKEY> <GRANTEE_KEYPAIR_FILE> [DELEGATES_KEYPAIR_FILES...]"
    echo "  SOLANA_RPC: The solana RPC url or moniker to use"
    echo "  GRANTOR_PUBKEY: The authority pubkey that issued the authorization"
    echo "  GRANTEE_KEYPAIR_FILE: The keypair that is the receiver of the authorization"
    echo "  DELEGATES_KEYPAIR_FILES: A list of keypair files that should be added as delegate to the authorization"
    exit 1
}

# Parse all our input values
if [[ "$#" -lt 3 ]]; then
    _usage
fi

SOLANA_RPC="$1"
shift

GRANTOR_PUBKEY="$1"
shift

GRANTEE_KEYPAIR_FILE="$1"
shift

if [[ ! -f "$GRANTEE_KEYPAIR_FILE" ]]; then
    echo "Error: Grantee keypair file '$GRANTEE_KEYPAIR_FILE' not found."
    _usage
fi
GRANTEE_PUBKEY=$(solana-keygen pubkey $GRANTEE_KEYPAIR_FILE)

DELEGATES_KEYPAIR_FILES=()
while [[ "$#" -gt 0 ]]; do
    DELEGATES_KEYPAIR_FILES+=("$1")
    shift
done

# Generate our list of public keys to be added as delegates
DELEGATES_PUBKEYS=()
for delegated_keypair_file in "${DELEGATES_KEYPAIR_FILES[@]}"; do
    DELEGATES_PUBKEYS+=($(solana-keygen pubkey $delegated_keypair_file))
done
DELEGATES_JSON_VALUES="["
for ((i = 0; i < ${#DELEGATES_PUBKEYS[@]}; i++)); do
    DELEGATES_JSON_VALUES+="\"${DELEGATES_PUBKEYS[$i]}\""
    if [[ $i -lt $((${#DELEGATES_PUBKEYS[@]} - 1)) ]]; then
        DELEGATES_JSON_VALUES+=","
    fi
done
DELEGATES_JSON_VALUES+="]"

# Constants
PSYCHE_AUTHORIZER_ID="PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw"
PSYCHE_AUTH_SCOPE="utf8:CoordinatorJoinRun"

# Make sure all is good to go
echo "SOLANA_RPC: $SOLANA_RPC"
echo "GRANTOR_PUBKEY: $GRANTOR_PUBKEY"
echo "GRANTEE_KEYPAIR_FILE: $GRANTEE_KEYPAIR_FILE"
echo "GRANTEE_PUBKEY: $GRANTEE_PUBKEY"
echo "DELEGATES_KEYPAIR_FILES: $DELEGATES_KEYPAIR_FILES"
echo "DELEGATES_PUBKEYS: $DELEGATES_PUBKEYS"
echo "DELEGATES_JSON_VALUES: $DELEGATES_JSON_VALUES"
echo "PSYCHE_AUTHORIZER_ID: $PSYCHE_AUTHORIZER_ID"
echo "PSYCHE_AUTH_SCOPE: $PSYCHE_AUTH_SCOPE"

# Find how the authorization was created and simulate it
AUTHORIZATION_CREATE_JSON=$( \
        solana-toolbox --rpc=$SOLANA_RPC instruction \
        $PSYCHE_AUTHORIZER_ID authorization_create \
        grantor:$GRANTOR_PUBKEY \
        --args=params.grantee:$GRANTEE_PUBKEY \
        --args=params.scope:$PSYCHE_AUTH_SCOPE
)

# Extract the authorization PDA from the JSON response
AUTHORIZATION_PUBKEY=$(echo $AUTHORIZATION_CREATE_JSON | jq -r .resolved.addresses.authorization)
echo "AUTHORIZATION_PUBKEY: $AUTHORIZATION_PUBKEY"

# Then we add the delegate keys to our list of delegates
echo "----"
echo "Setting delegates..."
AUTHORIZATION_DELEGATES_JSON=$( \
        solana-toolbox --rpc=$SOLANA_RPC instruction \
        $PSYCHE_AUTHORIZER_ID authorization_grantee_update \
        payer:keypair \
        grantee:$GRANTEE_KEYPAIR_FILE \
        authorization:$AUTHORIZATION_PUBKEY \
        --args=params.delegates_clear:true \
        --args=params.delegates_added:$DELEGATES_JSON_VALUES \
        --execute
)
echo $AUTHORIZATION_DELEGATES_JSON | jq -r .outcome.explorer_url
echo "----"
