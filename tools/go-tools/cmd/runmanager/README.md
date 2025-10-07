## Setup

```bash
cd tools/go-tools/cmd/runmanager
go mod download
go build -o runmanager
```

## Usage

```bash
./runmanager --run-id <RUN_ID> --rpc <RPC_URL> --ws-rpc <WS_RPC_URL> --wallet-path <WALLET_PATH> [OPTIONS]

# For example
./runmanager --run-id test --rpc http://nous-gpu-3:8899 --ws-rpc ws://nous-gpu-3:8900 --wallet-path ~/keys/client1
```
