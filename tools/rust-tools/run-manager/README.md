This binary is a manager for Psyche client containers. It allows users to connect to the Psyche network without having to worry about client versions, as it performs the necessary checks beforehand.

One can run the run manager like this:

```bash
cargo run --release -p run-manager -- --env-file .env.local
```

In case you already have a prebuilt binary:

```bash
./run-manager --env-file .env.local
```

Where:

- `--env-file` should point to a `.env` file where several relevant environment variables should be defined, for example:

  ```
  RPC=http://some-host:8899
  WS_RPC=ws://some-host:8900
  RUN_ID=the_run_id
  WALLET_PRIVATE_KEY_PATH=keys/keypair.json  # Optional
  ```

  - If `WALLET_PRIVATE_KEY_PATH` is defined, it will use the specified keypair instead of the default `$HOME/.config/solana/id.json`

The run manager will also try to restart the client a few times in case it encounters an error. If you notice that it is stuck, you may close the process manually via Ctrl+C and run it again.
