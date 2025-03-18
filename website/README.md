# psyche website

## frontend

a simple react app.

- displays info from the backend and allows viewing the state of runs
- allows people to connect a wallet and perform interactions with the Psyche contract(s).

### env vars

- `VITE_BACKEND_PORT`: Port of the backend's server. `3000` when running locally.
- `VITE_BACKEND_PATH`: Path of the backend's server. empty when running locally.
- `VITE_MINING_POOL_RPC`: URL (revealed publicly!) of the RPC for the mining pool contract to use on the frontend.

## backend

a chain indexer.

- indexes the state of the psyche coordinators & stores it in a DB
- indexes the state of the contribution contract & stores it in a DB

### env vars

- `PORT`: which port to run on.
- `CORS_ALLOW_ORIGIN`: if empty, always allowed. if passed, only allows requests from that origin.
- `GITCOMMIT`: used for the status page. set to the current git commit.
- `COORDINATOR_RPC`: which chain RPC to hit for the coordinator's state.
- `COORDINATOR_PROGRAM_ID`: the on-chain address of the coordinator program.
- `MINING_POOL_RPC`: which chain RPC to hit for the mining pool's state.
- `MINING_POOL_PROGRAM_ID`: the on-chain address of the mining pool program.

## running locally

### setting up a localnet run for data

1. start `solana-test-validator --limit-ledger-size 10000000` in another terminal.
2. deploy a run to the localnet. locally, you probably want to use a small model, so do `just setup-solana-localnet-light-test-run RUN_ID --name "\"silly run name\"" --description "\"this is a test run set up locally. it's used for training a silly model.\"" --num-parameters 12345678`
3. start training your run! `just start-training-localnet-light-client RUN_ID` in another terminal.

### running with the backend pointed to localnet

1. `cd backend`, `pnpm dev-local` in another terminal. This will build the WASM for deserializing the onchain state, build the IDL for interacting with the contracts, and start the backend.
2. `cd frontend`, `pnpm dev` in another terminal.

### running with the backend pointing to a non-localnet setup

to use the devnet for the mining pool, while using a local coordinator,
set env var MINING_POOL_RPC, run `pnpm dev-local-and-devnet` in the backend.
