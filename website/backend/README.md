# psyche backend

## Running on local testnet

1. Install deps

```bash
pnpm i
```

2. Make sure you have a solana wallet:

```bash
ls ~/.config/solana/id.json
```

if you don't, make one:

```bash
solana-keygen new
```

3. Start a local solana validator and deploy the programs in it:

```bash
scripts/setup-and-deploy-solana-test.sh
```

4. Start a training node to create dummy transactions:

```bash
scripts/train-solana-test.sh
```

5. Start the website backend:

```bash
pnpm dev-local
```
