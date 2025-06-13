#!/bin/bash

set -o errexit
set -o pipefail

cd ../architectures/decentralized/solana-coordinator
coordinator_address=$(grep -r --include='*.rs' 'declare_id!(' . | sed -n 's/.*declare_id!(\([^)]*\)).*/\1/p')
cd ../solana-authorizer
authorizer_address=$(grep -r --include='*.rs' 'declare_id!(' . | sed -n 's/.*declare_id!(\([^)]*\)).*/\1/p')

if [ -z "$coordinator_address" ]; then
    echo "Error: No declare_id! macro found for coordinator."
    exit 1
fi

if [ -z "$authorizer_address" ]; then
    echo "Error: No declare_id! macro found for authorizer."
    exit 1
fi

echo -e "\nCoordinator address: ${coordinator_address}\n"
echo -e "Authorizer address: ${authorizer_address}\n"
echo -e "These are the addresses that will be used in the Psyche client binary"
read -p "Continue? [y/N] " answer

if [[ "$answer" =~ ^[Yy]$ || -z "$answer" ]]; then
    echo "Continuing..."
else
    echo "Exiting."
    exit 1
fi
