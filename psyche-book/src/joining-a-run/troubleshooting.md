# Troubleshooting

This guide covers common issues when joining and running a Psyche client.

## Docker & GPU Access

### Issue: Docker can't access GPU

**Symptoms:**

- Error: "could not select device driver with capabilities: [[gpu]]"
- Error: "no CUDA-capable device is detected"
- Container starts but can't find GPU

**Solutions:**

1. **Verify NVIDIA Container Toolkit is installed:**

   ```bash
   docker run --rm --gpus all nvidia/cuda:11.8.0-base-ubuntu22.04 nvidia-smi
   ```

   If this fails, reinstall the toolkit: [Installation Guide](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html)

2. **Check Docker daemon configuration:**

   ```bash
   cat /etc/docker/daemon.json
   ```

   Should include:

   ```json
   {
   	"runtimes": {
   		"nvidia": {
   			"path": "nvidia-container-runtime",
   			"runtimeArgs": []
   		}
   	}
   }
   ```

3. **Restart Docker daemon:**

   ```bash
   sudo systemctl restart docker
   ```

4. **Verify NVIDIA drivers are loaded:**
   ```bash
   nvidia-smi
   ```
   If this fails, your NVIDIA drivers aren't properly installed.

### Issue: Out of Memory (OOM) Errors

**Symptoms:**

- Container crashes with exit code 137
- Error: "CUDA out of memory"
- Docker logs show memory allocation failures

**Solutions:**

1. **Reduce MICRO_BATCH_SIZE:**
   Edit your `.env` file:

   ```env
   MICRO_BATCH_SIZE=2  # Try reducing from 4 to 2, or even 1
   ```

2. **Check VRAM availability:**

   ```bash
   nvidia-smi
   ```

   Look at "Memory-Usage" column. Training large models requires significant VRAM.

3. **Increase TENSOR_PARALLELISM (multi-GPU only):**
   If the model doesn't fit on one GPU:

   ```env
   TENSOR_PARALLELISM=2  # Split model across 2 GPUs
   ```

4. **Close other GPU applications:**
   Make sure no other programs are using your GPU during training.

**VRAM Requirements by Model Size:**

- Small models (20M-100M params): 4-8GB VRAM
- Medium models (100M-1B params): 8-16GB VRAM
- Large models (1B-7B params): 16-40GB VRAM
- Very large models (7B+ params): 40GB+ VRAM or multi-GPU setup

### Issue: Container starts then immediately exits

**Symptoms:**

- `docker ps` shows no running container
- Container appears briefly then disappears

**Solutions:**

1. **Check logs for error messages:**

   ```bash
   docker logs psyche-client
   ```

2. **Common causes:**
   - Missing or invalid `.env` file
   - Incorrect wallet private key format
   - Missing environment variables

3. **Verify your .env file:**

   ```bash
   cat ~/psyche-client.env
   ```

   Ensure all required variables are set (RPC, WS_RPC, RUN_ID, etc.)

4. **Check wallet file:**
   ```bash
   cat ~/psyche-wallet.json
   ```
   Should be valid JSON with a private key array.

## Solana RPC Issues

### Issue: RPC Connection Failures

**Symptoms:**

- Error: "failed to connect to RPC"
- Timeout errors in logs
- "Connection refused" messages

**Solutions:**

1. **Verify RPC URLs are correct:**

   ```bash
   curl -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1, "method":"getHealth"}' YOUR_RPC_URL
   ```

   Should return `{"jsonrpc":"2.0","result":"ok","id":1}`

2. **Check RPC provider status:**
   - Visit your RPC provider's status page
   - Try the fallback RPC_2 endpoint

3. **Test WebSocket connection:**

   ```bash
   websocat YOUR_WS_RPC_URL
   ```

   Should connect without errors (Ctrl+C to exit).

4. **Rate limiting:**
   - Free RPC endpoints may have rate limits
   - Consider upgrading to a paid plan for production use
   - Configure a reliable backup RPC_2

5. **Network connectivity:**
   ```bash
   ping google.com
   ```
   Verify your internet connection is stable.

### Issue: Transaction Simulation Failed

**Symptoms:**

- Error: "Transaction simulation failed"
- "Blockhash not found" errors
- Transaction rejected

**Solutions:**

1. **RPC node may be behind:**
   - Switch to a different RPC provider
   - Use a premium RPC service with better reliability

2. **Check Solana network status:**
   - Visit https://status.solana.com/
   - Network may be experiencing degraded performance

3. **Retry the operation:**
   - Many transaction failures are transient
   - The client will usually retry automatically

### Issue: Wallet Needs SOL for Fees

**Symptoms:**

- Error: "insufficient funds for transaction fee"
- Transaction fails due to low balance

**Solutions:**

For most runs, you **don't need SOL** as a participant - the coordinator handles transactions. However, if you need SOL:

1. **Check your balance:**

   ```bash
   solana balance ~/psyche-wallet.json
   ```

2. **Get SOL from a faucet (devnet only):**

   ```bash
   solana airdrop 1 ~/psyche-wallet.json
   ```

3. **Transfer SOL (mainnet):**
   Use a wallet application to send SOL to your address.

## Network & P2P Connectivity

### Issue: Can't Connect to Other Clients

**Symptoms:**

- Training doesn't progress past warmup
- "Peer connection timeout" in logs
- No P2P messages received

**Solutions:**

1. **Verify network connectivity:**

   ```bash
   curl ifconfig.me  # Check you can reach the internet
   ```

2. **Firewall configuration:**
   Psyche uses P2P networking. Ensure your firewall allows:
   - Outbound connections on all ports
   - Inbound connections for P2P (if behind NAT, UPnP may help)

3. **Check Docker network mode:**
   Must use `--network "host"` for P2P to work properly:

   ```bash
   docker inspect psyche-client | grep NetworkMode
   ```

   Should show `"NetworkMode": "host"`

4. **NAT/Router issues:**
   - If behind NAT, ensure UPnP is enabled on your router
   - Or configure port forwarding (varies by run configuration)

### Issue: Slow or No Training Progress

**Symptoms:**

- Rounds taking much longer than expected
- Client stuck in one phase
- No progress messages in logs

**Solutions:**

1. **Check GPU utilization:**

   ```bash
   nvidia-smi -l 1  # Monitor GPU usage every second
   ```

   GPU usage should be high (>80%) during training.

2. **Verify data provider access:**
   - Check logs for data download errors
   - Ensure network bandwidth is sufficient for data fetching
   - GCS/HTTP endpoints must be reachable

3. **Monitor network bandwidth:**

   ```bash
   iftop  # Or use `nethogs` to see network usage
   ```

   P2P model sharing requires good bandwidth (10+ Mbps recommended).

4. **Check coordinator state:**
   The run may be legitimately slow if:
   - Few clients are participating
   - Waiting for minimum clients to join
   - Coordinator is paused

## Authorization Issues

### Issue: "Not Authorized to Join Run"

**Symptoms:**

- Error: "authorization check failed"
- "Wallet not authorized for this run"
- Join request rejected

**Solutions:**

1. **Verify authorization:**

   ```bash
   psyche-solana-client can-join \
       --run-id YOUR_RUN_ID \
       --authorizer YOUR_AUTHORIZER \
       --wallet $(solana-keygen pubkey ~/psyche-wallet.json)
   ```

2. **Check AUTHORIZER in .env:**
   Must match the public key that authorized your wallet.

3. **For permissioned runs:**
   - Contact run owner to authorize your wallet
   - If using delegate keys, ensure they're properly set

4. **Verify you're using the correct run ID:**
   Double-check the RUN_ID in your .env file.

### Issue: Wallet File Errors

**Symptoms:**

- "Cannot read wallet" or "Invalid wallet format"
- "Failed to parse private key"

**Solutions:**

1. **Check wallet file format:**

   ```bash
   cat ~/psyche-wallet.json
   ```

   Should be a JSON array of numbers: `[123,45,67,...]`

2. **Regenerate wallet if corrupted:**

   ```bash
   solana-keygen new -o ~/psyche-wallet-new.json
   ```

   Then get re-authorized for the run.

3. **Check file permissions:**
   ```bash
   ls -la ~/psyche-wallet.json
   ```
   Ensure the file is readable.

## Performance Issues

### Issue: Training Slower Than Expected

**Possible causes and solutions:**

1. **Suboptimal parallelism settings:**
   - Review GPU configuration in .env
   - For multi-GPU: increase DATA_PARALLELISM
   - Monitor GPU utilization to verify GPUs are being used

2. **Network bottleneck:**
   - P2P model sharing requires bandwidth
   - Check network latency: `ping 8.8.8.8`
   - Consider upgrading internet connection

3. **CPU bottleneck:**
   - Data loading can be CPU-intensive
   - Monitor CPU usage: `htop`
   - Ensure sufficient CPU cores available

4. **Disk I/O:**
   - Model checkpoints stored on slow disk
   - Use SSD for better performance
   - Check disk usage: `iostat -x 1`

### Issue: Client Keeps Getting Ejected

**Symptoms:**

- Removed from epoch repeatedly
- "Health check failed" in logs
- Marked as unhealthy by coordinator

**Solutions:**

1. **Check internet stability:**
   - Intermittent connection causes ejection
   - Run continuous ping test: `ping -c 100 8.8.8.8`

2. **Verify GPU isn't overheating:**

   ```bash
   nvidia-smi -q -d TEMPERATURE
   ```

   If temperature >85°C, improve cooling.

3. **Ensure sufficient resources:**
   - CPU usage not at 100%
   - Enough RAM available
   - GPU not shared with other processes

4. **Check training speed:**
   - If too slow, may not complete rounds in time
   - Reduce MICRO_BATCH_SIZE to speed up
   - Or upgrade hardware

## Common Error Messages

### "Coordinator state mismatch"

**Meaning:** Client state out of sync with coordinator.

**Solution:**

- Wait for next epoch to rejoin
- Or restart client: `docker restart psyche-client`

### "Failed to download model checkpoint"

**Meaning:** Cannot access model from HuggingFace or specified source.

**Solutions:**

- Check internet connectivity
- Verify HuggingFace Hub is accessible: `curl https://huggingface.co`
- Check if model repo exists and is public
- Wait and retry - may be transient failure

### "Witness quorum not reached"

**Meaning:** Not enough witnesses submitted proofs (coordinator-side issue).

**Impact:**

- Epoch may take longer to complete
- Training will continue after timeout

**What you can do:**

- Nothing - this is a run-level issue
- If persistent, notify run owner

### "Bloom filter verification failed"

**Meaning:** Training results don't match expected commitments.

**Possible causes:**

- Network corruption during P2P transfer
- Client software bug

**Solution:**

- Usually handled automatically by retrying
- If persistent, restart client
- Report to Psyche team if it continues

## Getting Additional Help

If your issue isn't covered here:

1. **Check FAQ:** [Client FAQ](./faq.md) for additional questions
2. **Review Requirements:** [Requirements](./requirements.md) to verify setup
3. **Search GitHub Issues:** [Psyche Repository](https://github.com/PsycheFoundation/psyche/issues)
4. **Check logs carefully:** Often contain specific error messages
5. **Join community:** Look for Psyche Discord or community channels

## Diagnostic Commands

Gather information for bug reports:

```bash
# System information
uname -a
nvidia-smi
docker --version
docker info | grep -i runtime

# Container status
docker ps -a | grep psyche
docker logs psyche-client --tail 100

# Client state
psyche-solana-client json-dump-user \
    --rpc YOUR_RPC \
    --run-id YOUR_RUN_ID \
    --wallet $(solana-keygen pubkey ~/psyche-wallet.json)
```

Include this information when reporting issues.
