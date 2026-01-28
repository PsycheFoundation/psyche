# Witness Phase

## Witness phase (state: RoundWitness)

This phase exists to give the witnesses an opportunity to send their proofs to the Coordinator in the event that they have not received enough training results from other clients to have reached the quorum and send their proofs opportunistically.

There is also brief slack period for non-witness nodes to catch up by downloading any remaining results they might have not received.

When the _Witness_ phase finishes via timeout, the Coordinator transitions from _Witness_ to the _Cooldown_ phase in three cases:

- If we are in the last round of the epoch.
- If the clients have dropped to less than the minimum required by the config.
- If the number of witnesses for the round is less than the quorum specified by the config.

Any clients that have failed health checks will also be removed from the current epoch.
