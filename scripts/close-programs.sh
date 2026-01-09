echo -e "\n[+] Closing deployed Solana programs from devnet, are you sure? (y/N)"
read -r CONFIRMATION
if [[ "$CONFIRMATION" != "y" && "$CONFIRMATION" != "Y" ]]; then
    echo "Aborting."
    exit 0
fi
solana program close $(solana-keygen pubkey architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json) --bypass-warning --url devnet
solana program close $(solana-keygen pubkey architectures/decentralized/solana-authorizer/target/deploy/psyche_solana_authorizer-keypair.json) --bypass-warning --url devnet
solana program close $(solana-keygen pubkey architectures/decentralized/solana-treasurer/target/deploy/psyche_solana_treasurer-keypair.json) --bypass-warning --url devnet
