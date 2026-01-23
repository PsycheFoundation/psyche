# Psyche Solana Mining Pool

This smart contract provides a resource-pooling mechanism for users to team together in order to raise funds for a training run.

## How it works

On a high level, the flow of a Mining Pool is the following:

1. A "Delegate" creates a Mining Pool to raise funds
2. Any "User" contributes collateral to the Mining Pool
3. The "Delegate" withdraw the collateral and uses the collateral to buy compute infrastructure and then return rewards tokens to the Mining Pool
4. The "User" can then claim its share of the distributed reward tokens.

## Solana Instructions

To achieve this the smart contract provides the following capabilities (in order):

- `pool_create`, An "Authority" can create a `Pool` and specify its intent
- `pool_update`, The creator of the pool can update the pool's configuration
- `lender_create`, A "User" can create a `Lender` PDA for future contributions to a `Pool`
- `lender_deposit`, A "User" can contribute collateral to the pool of its choice.
- `pool_extract`, The creator of the `Pool` can withdraw the collateral deposited through all the `Lender`s in order to use that collateral to fund compute infrastructure.
- If the training has yielded reward tokens, the creator can deposit reward token into the `Pool`
- `pool_claimable`, The creator of the `Pool` can enable `Lender`s to claim the reward tokens
- `lender_claim`, A User can then claim the reward token on its previously created `Lender`
