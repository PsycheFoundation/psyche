```bash
cargo run --release -p run-manager -- --env-file .env.local --wallet-path keys/keypair.json
```

## Generating a release-ready binary

This will generate a stripped and optimized binary ready for distribution in `$PROJECT_ROOT/target/release-dist/`

```bash
cargo build --profile release-dist -p run-manager
```
