# Client FAQ

- Which operating systems are supported?
  - We officially support modern Linux versions. macOS is supported for development purposes only (not for production training).
- What are the hardware requirements to run a client?
  - Linux: You need a CUDA-compatible GPU. As for the exact specs this will depend on the size of the model being trained.
  - macOS (development only): Apple Silicon Macs can use Metal Performance Shaders for GPU acceleration.
  - Support for AMD ROCm is planned for the future.
- Can I join a run at any moment?
  - Yes! You will remain as a pending client and start training in the next epoch.
- Can I leave a run at any moment? How do I leave a run?
  - Yes, you may leave a run by closing the container with the client, either with Ctrl+C in the terminal or by stopping manually the container if it's running detached. However take into account that once rewards are implemented, you will lose all rewards for that epoch.
- What happens if my connection drops or my client crashes?
  - This is similar to closing the client, just make sure the docker container is correctly stopped and re-run the client.
- How do I update the client to the latest version?
  - You can force Docker to pull the latest image by running `docker pull nousresearch/psyche-client:latest` before running the client.
- Do I need a Solana wallet to train? Does it need to have funds?
  - Yes, even if you want to join a run that does not track rewards you will need a Solana wallet with funds to pay for the transactions to the coordinator.
- Are the client and coordinator open-source? Can I report bugs?
  - Yes, you may check [Psyche's github repo](https://github.com/PsycheFoundation/psyche)
