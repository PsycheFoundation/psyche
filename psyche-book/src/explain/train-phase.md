# Train Phase

## Training (state: RoundTrain)

In this phase, the Coordinator provides a random seed.

Each client can use this seed, alongside the current round index and epoch index to determine which indices of the training data to use.

Each client then proceeds to run the training on the selected training data.

This state will end when clients later exchanges `Witness` messages.

### Witnessing training results

As clients complete their training, they send their results to all other clients, including the Witnesses. The witnesses will each send a **witness proof** to the Coordinator, building towards a **witness quorum**.

A witness proof contains a bloom filter describing which pieces of data the witness received training results for, and which clients did that work. Elected witnesses are responsible for creating these witness proofs and and sending them to the Coordinator.

The witnesses for each round are chosen randomly from all the clients, using the same random seed as for data assignments. A witness will attempt to send an **opportunistic witness** message once it's seen a received a training result for every single batch in the current round.

### Witness Quorum

The Coordinator advances the run from the _Training_ phase to the _Witness_ phase in one of two ways:

- If enough witnesses observe all results and reach a **witness quorum** for the round, they notify the Coordinator that it is safe to advance. This process, named **opportunistic witnessing**, accelerates the transition to the _Witness_ phase, rather than having to wait a fixed time for training results.
- If witnesses do not receive all required results from other clients before the maximum time specified for the _Training_ phase, the Coordinator will nonetheless transition to the _Witness_ phase after the maximum _Training_ time elapses.
