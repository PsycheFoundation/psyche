# Training Rewards

When clients participate in a training run, the `Coordinator` keeps track of the compute contributions.

// TODO (vbrunet) - update docs
An `earning_rate` is added to the Client's `earned` points at the end of every successful training Epoch.

## Run Treasurer, Compute Incentives

A Training run can be created through a `Treasurer` escrow smart contract. In this case the `Run`'s authority will be the `Treasurer` smart contract itself.

In this case, an arbitrary token can be distributed through the `Treasurer`'s Token holding. Every time a client earns a point on the run's coordinator, the treasurer will allow claiming a fixed amount of reward token for each earned coordinator point.

The source code for the treasurer smart contract can be found here: <https://github.com/PsycheFoundation/psyche/tree/main/architectures/decentralized/solana-treasurer>.

## Mining Pool, Pooling funds

Participating in a run can be expensive — a powerful GPU may be required to train a particular model. Users can pool resources together through a Mining Pool smart contract. The source code used can be found here: <https://github.com/PsycheFoundation/psyche/tree/main/architectures/decentralized/solana-mining-pool>.

Each user contributing to a Mining Pool will delegate their funds so those can be used by the Mining Pool authority and owner to purchase compute power. The Mining Pool authority can then re-distribute equitably through the Mining Pool any token that may have been received as a result of the training.
