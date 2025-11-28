# Releases

For each Psyche client version release, we will have to update at least two variables in the code:

- In `docker.nix` the `tag` variable associated to the client docker image should be changed to the new version.
- In `architectures/decentralized/solana-client/src/app.rs`, the `CLIENT_VERSION` constant should be changed to the new version.

[!] Both should be updated at the same time, if not these could lead to inconsistent behaviour

Once the new version has been released and the new docker image uploaded to DockerHub, you can update the client version required
for a particular run. Note that for this, you should be the run owner:

```bash
cargo run --release --bin psyche-solana-client \
    -- update-client-version \
    --wallet-private-key-path <path_to_run_owner_private_key> \
    --run-id <your_run_id> \
    --new-version <new_version>
```
