# Psyche Solana Distributor

This smart contract provides a way to efficiently distribute token allocations amongst a list of receiver, with customizable vesting periods.

## How it works

On a high level, the flow of an distribution is the following:

1. An authority wallet creates an `Airdrop` PDA with the list of recipients hash
2. The authority wallet funds the `Airdrop`'s collateral vault with tokens
3. Users can claim their share if they can proove that they are a valid recipient

Internally, the `Airdrop` contains the hash of the root of a merkle tree of all the token allocations (users/amounts). In order for users to proove that they are indeed a valid recipient of some distributed tokens, users must submit a merkle-proof of their own token allocation (recipient address / amount / vesting info). This proof will be checked against the merkle tree root during redeem operations.

## Solana Instructions

To achieve this the smart contract provides the following capabilities (in order):

- `airdrop_create`, An authority can create a `Airdrop` and specify its intent
- `airdrop_update`, The authority of the `Airdrop` can change its configuration
- `claim_create`, Users can create a `Claim` PDA, that can be used to redeem later
- `claim_redeem`, Users can claim their vested token, by providing a valid proof
- `airdrop_withdraw`, The authority can clawback any unclaimed token
