# psyche backend

## Running on local testnet

1. install deps

```bash
pnpm i
```

2. make sure you have a solana wallet:

```bash
ls ~/.config/solana/id.json
```

if you don't, make one:

```bash
solana-keygen new
```

3. start a local solana testnet: in another terminal,

```bash
solana-test-validator
```

4. deploy the programs to testnet: in another terminal,

```bash
scripts/deploy-solana-test.sh
scripts/train-solana-test.sh
```

5. start the website backend:

```bash
pnpm dev-local
```
