```bash
cargo run --release -p run-manager -- --env-file .env.local --wallet-path keys/keypair.json
```

## Generating a release-ready binary

This will generate a binary ready for distribution in `$PROJECT_ROOT/target/release/`

```bash
cargo build --profile release -p run-manager
```
