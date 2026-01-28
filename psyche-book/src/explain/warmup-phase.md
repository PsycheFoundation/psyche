# Warmup Phase

## Model Loading (state: Warmup)

This phase is designed to let all clients download the model & load it onto their GPUs.

If a client has dropped whilst waiting for the warmup time, the Backend then removes the client from the Coordinator's clients list.

If the number of clients falls below min_clients, the Coordinator goes back to the `WaitingForMembers` phase.

Once the `Warmup` time passes, the Coordinator loads all the information for the next training round and change its phase to `RoundTrain`. The Server will broadcast this `Training` Coordinator state to all clients.
