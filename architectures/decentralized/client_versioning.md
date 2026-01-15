# Client versioning in Runs

There are two ways to specify the client version for a run:

- Via the docker `RepoId` hash
- Via a docker version tag

## Docker RepoId hash

Once the client docker image is uploaded to DockerHub, a `RepoId` hash is associated with that image. This string is what should be used for
setting the client version in a run, toguether with the "sha256" part. For example, "sha256:ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb".

## Docker version tag

For setting a docker version tag, the image should be built with that tag set beforehand. This should be done in the `docker.nix` file, changing the `tag` field in the `docker-psyche-solana-client` docker package.

## Updating client docker version for a run

Once the new docker image uploaded to DockerHub and some version selected, you can update the client version required
for a particular run with the following command:

[!] You should have the run owner solana key to successfully run this command
[!] The run must be paused beforehand to do the client version update

```bash
cargo run --release --bin run-manager -- \
	update-config \
	--wallet-private-key-path <path_to_run_owner_private_key> \
	--run-id <your_run_id> \
	--client-version <new_version>
```
