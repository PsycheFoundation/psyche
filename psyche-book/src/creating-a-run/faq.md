# FAQ - Run Owners

Common questions for creators and managers of Psyche training runs.

## Cost & Economics

### How much does it cost to create and run a training run?

**Solana Transaction Costs:**

- Creating a run: ~0.001-0.01 SOL (varies with network congestion)
- Updating configuration: ~0.001 SOL per update
- Pausing/resuming: ~0.001 SOL per transaction
- Ticking the coordinator: Handled automatically by clients or other users

**RPC Costs:**

- Free public endpoints exist but may be unreliable
- Paid RPC providers: $10-50/month for basic plans, more for high-volume
- Self-hosted RPC node: Server costs ($50-200/month depending on specs)

**Optional Token Rewards:**

- If you're distributing rewards, you'll need to fund the treasury
- Cost depends on earning rates and number of participants
- Can be topped up as needed

**Total minimum to get started:** ~1 SOL + RPC provider

### How many clients do I need for a successful run?

**Minimum requirements:**

- Set by `min_clients` and `init_min_clients` in your config
- Can be as low as 1 for testing, but not practical for real training

**Recommended minimums:**

- **Testing/Development:** 2-3 clients
- **Small runs:** 5-10 clients
- **Production runs:** 10+ clients for redundancy and speed

**Considerations:**

- More clients = faster training (more data batches processed in parallel)
- More clients = better fault tolerance (some can drop without stopping the run)
- Set `init_min_clients` higher than `min_clients` for stable starts
- More clients requires higher `global_batch_size` settings

### How do reward rates work?

**Points System:**

- Clients earn points for successfully completing epochs
- Points are distributed equally among all clients that finish an epoch
- Clients can lose points (slashing) for bad behavior or failures

**Setting Rates:**

```bash
psyche-solana-client set-future-epoch-rates \
    --earning-rate 100 \    # Points earned per epoch
    --slashing-rate 50      # Points lost for failures
```

**Converting to Tokens:**

- If using a treasurer, points can be claimed for tokens
- Exchange rate is determined by treasury funding and total points
- Example: 10,000 tokens in treasury ÷ 1,000 total points = 10 tokens per point

**Best Practices:**

- Start with conservative rates
- Monitor point accumulation
- Adjust rates with `set-future-epoch-rates` (applies to future epochs only)
- Fund treasury regularly to ensure claimable rewards

## Run Management

### Can I pause and resume a run?

**Yes!** Use the `set-paused` command:

**To pause:**

```bash
psyche-solana-client set-paused \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --pause \
    --wallet-private-key-path [WALLET_PATH]
```

**To resume:**

```bash
psyche-solana-client set-paused \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --resume \
    --wallet-private-key-path [WALLET_PATH]
```

**When paused:**

- No new clients can join
- Current epoch completes normally
- After cooldown, run stays in WaitingForMembers until resumed

**Common reasons to pause:**

- Updating configuration parameters
- Investigating issues with the run
- Taking a break before continuing training
- Performing maintenance on infrastructure

### Can I update the configuration of a running run?

**Yes, but with caveats:**

**Configuration that CAN be updated on the fly:**

```bash
psyche-solana-client update-config \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --config-path [NEW_CONFIG_PATH] \
    --wallet-private-key-path [WALLET_PATH]
```

Changes take effect at the start of the next epoch.

**What's safe to change:**

- Timing parameters (`warmup_time`, `cooldown_time`, `max_round_train_time`)
- Client thresholds (`min_clients`, `init_min_clients`)
- Batch sizes (`global_batch_size_start`, `global_batch_size_end`)
- Witness settings (`witness_nodes`)

**What should NOT be changed mid-run:**

- Model architecture or size
- Checkpoint locations (may cause issues)
- Data provider configuration (clients may have cached data)

**Best practice:**

- Test config changes on a separate run first
- Pause the run before making major changes
- Allow current epoch to complete before changes take effect

### How do I monitor my run's health?

**Inspect overall run state:**

```bash
psyche-solana-client json-dump-run \
    --rpc [RPC] \
    --run-id [RUN_ID]
```

**Key metrics to monitor:**

- `state`: Current phase (WaitingForMembers, Warmup, RoundTrain, etc.)
- `epoch`: Current epoch number
- `round`: Current round within epoch
- `clients`: List of participating clients and their states

**Check specific client:**

```bash
psyche-solana-client json-dump-user \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --wallet [CLIENT_PUBKEY]
```

**Watch for:**

- Clients frequently being ejected (health check failures)
- Epochs taking much longer than expected
- Run stuck in one state for extended period
- Low client participation despite authorizations

**Monitoring tools:**

- Set up regular polling of `json-dump-run`
- Monitor on-chain events via RPC subscriptions
- Track client-reported metrics
- Check reward accumulation to verify training is progressing

### What happens if clients drop during an epoch?

**During Training:**

- Round continues if remaining clients ≥ `min_clients`
- If clients drop below `min_clients`, coordinator transitions to Cooldown
- Dropped clients lose rewards for that epoch

**Health Check System:**

- Clients send regular health checks
- Clients report other clients they consider unhealthy
- Coordinator tracks health scores based on witness proofs
- Unhealthy clients are ejected before next round

**Recovery:**

- Run moves to Cooldown → WaitingForMembers
- Waits for `init_min_clients` to join
- Training resumes from last checkpoint

**Minimizing disruptions:**

- Set reasonable `min_clients` (not too high)
- Configure longer `max_round_train_time` for slower GPUs
- Monitor client health proactively
- Maintain backup clients if possible

## Authorization & Access Control

### Should I make my run permissioned or permissionless?

**Permissionless Runs (Anyone can join):**

**Pros:**

- Easier to get participants
- More decentralized
- No authorization management overhead

**Cons:**

- No control over who joins
- Potential for malicious actors
- May have quality/consistency issues

**Create universal authorization:**

```bash
sh scripts/join-authorization-create.sh [RPC] join_authority.json 11111111111111111111111111111111
```

**Use cases:**

- Public training projects
- Open research initiatives
- Maximum decentralization desired

**Permissioned Runs (Specific users only):**

**Pros:**

- Control who participates
- Better security and trust
- Can vet participant hardware
- Easier to coordinate

**Cons:**

- Manual authorization management
- Fewer potential participants
- Less decentralized

**Create per-user authorization:**

```bash
sh scripts/join-authorization-create.sh [RPC] join_authority.json [USER_PUBKEY]
```

**Use cases:**

- Private training runs
- Enterprise deployments
- Research with trusted partners
- Quality/performance requirements

**Hybrid approach:**

- Start permissioned for testing
- Open up gradually as confidence grows
- Whitelist known good participants first

### How do I add or remove authorized clients?

**Adding new users (permissioned runs):**

1. **Create authorization for new user:**

   ```bash
   sh scripts/join-authorization-create.sh [RPC] join_authority.json [NEW_USER_PUBKEY]
   ```

2. **User sets up delegate keys** (if needed):
   ```bash
   sh scripts/join-authorization-set-delegates.sh devnet [YOUR_PUBKEY] user.json delegate1.json delegate2.json
   ```

**Revoking access:**

Currently, there's no direct revoke mechanism. To remove a user:

1. They must voluntarily leave (stop their client)
2. If they misbehave, they'll be automatically ejected by health checks
3. For next run, don't re-authorize them

**Managing delegate keys:**

Users can manage their own delegate keys:

```bash
sh scripts/join-authorization-set-delegates.sh [NETWORK] [GRANTOR_PUBKEY] grantee.json delegate*.json
```

This allows users to:

- Authorize multiple machines/wallets
- Rotate keys for security
- Manage data center clusters

**Best practices:**

- Keep a list of authorized users
- Document authorization dates
- Review authorizations periodically
- Use descriptive pubkey labels/notes

### Can I have multiple run owners/administrators?

**Limitations:**

- Only one `main_authority` can modify run configuration
- `main_authority` is set at run creation and cannot be changed
- `join_authority` is also set at creation

**Workarounds:**

1. **Shared wallet approach:**
   - Multiple people use the same private key file
   - **Not recommended** for security reasons
   - Lost if anyone loses the key

2. **Coordinator pattern:**
   - One person is official owner
   - Others can still:
     - Monitor run via `json-dump-run`
     - Manage their own client authorizations (if they're join_authority)
     - Tick the coordinator (anyone can do this)

3. **Multi-sig wallet (future):**
   - Not currently supported
   - Would require Solana program changes

**Delegation of responsibilities:**

- Run owner: Config updates, pausing/resuming
- Join authority: User authorization management
- Treasurer (if separate): Reward funding, rate setting
- Monitors: Anyone can observe via RPC

## Checkpoints & Data

### Where are model checkpoints stored?

**Three checkpoint modes:**

1. **HuggingFace Hub:**

   ```toml
   [model.LLM.checkpoint.Hub]
   repo_id = "username/model-name"
   ```

   - Checkpoints stored in HuggingFace repository
   - Requires model repo to be public (or access tokens configured)
   - Clients download from Hub at epoch start

2. **P2P (Peer-to-Peer):**

   ```toml
   [model.LLM.checkpoint]
   P2P = {}
   ```

   - Checkpoints shared between clients directly
   - No central storage needed
   - Faster for clients already in the run
   - New clients get checkpoints from existing participants

3. **Local:**

   ```toml
   [model.LLM.checkpoint.Local]
   path = "/path/to/checkpoint"
   ```

   - For development/testing only
   - Each client must have checkpoint locally

**Storage locations (on clients):**

- Downloaded checkpoints cached locally
- Stored in Docker container volume
- Reused across restarts if available

### How often should checkpoints be saved?

**Current behavior:**

- Checkpoints saved during Cooldown phase
- Frequency: Once per epoch
- Cannot currently configure custom checkpoint frequency

**Epoch frequency determined by:**

- `rounds_per_epoch`: Number of training rounds per epoch
- Round timing: `max_round_train_time` + `round_witness_time`
- Example: 20 rounds × 30 sec = 10 minutes between checkpoints (minimum)

**Trade-offs:**

**Frequent checkpoints (shorter epochs):**

- Pros: Less training lost if run fails, easier to resume
- Cons: More overhead, slower overall training, more cooldown time

**Infrequent checkpoints (longer epochs):**

- Pros: Less overhead, faster training throughput
- Cons: More work lost on failures, longer recovery

**Recommendations:**

- Development/testing: Short epochs (5-10 rounds)
- Production: Medium epochs (20-50 rounds)
- Adjust `rounds_per_epoch` in config based on model size and stability

### Can I use a custom dataset?

**Yes!** Psyche supports multiple data providers.

**Option 1: HTTP Data Provider**

Serve data via HTTP/HTTPS:

```toml
[model.LLM.data_location.Http]
token_size_in_bytes = "TwoBytes"
shuffle = "DontShuffle"

[model.LLM.data_location.Http.location.HttpUrls]
urls = ["https://yourserver.com/data/shard1", "https://yourserver.com/data/shard2"]
```

**Option 2: Google Cloud Storage (GCS)**

Store data in GCS bucket:

```toml
[model.LLM.data_location.Http.location.Gcp]
bucket_name = "your-bucket-name"
filter_directory = "tokenized-data"
```

**Option 3: Local Files** (testing only)

```toml
[model.LLM.data_location.Local]
path = "/path/to/data"
token_size_in_bytes = "TwoBytes"
shuffle = "DontShuffle"
```

**Data Format Requirements:**

- Pre-tokenized data
- Binary format (tokens as bytes)
- Organized into shards/batches
- Token size specified (TwoBytes for most models)

**Data Assignment:**

- Coordinator assigns batch IDs deterministically
- Each client fetches assigned batches
- No overlap - each batch trained exactly once per epoch

See [Data Provider](../explain/data-provider.md) for details.

## Rewards & Treasury

### How do I fund the reward treasury?

**Top up treasury with tokens:**

```bash
psyche-solana-client treasurer-top-up-rewards \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --collateral-amount [AMOUNT_IN_SMALLEST_UNIT] \
    --wallet-private-key-path [WALLET_PATH]
```

**Amount calculation:**

- Specify in token's smallest unit (like lamports for SOL)
- Example: For token with 6 decimals, 1000000 = 1 token

**Treasury planning:**

- Estimate points per epoch: earning_rate × clients
- Multiply by expected epochs
- Add buffer (20-30%) for safety

**Example calculation:**

```
Earning rate: 100 points/epoch
Expected clients: 10
Expected epochs: 100

Total points = 100 × 10 × 100 = 100,000 points
Token budget = 100,000 tokens (1:1 exchange)
With 30% buffer = 130,000 tokens needed
```

**Monitoring:**

- Check treasury balance periodically
- Monitor claim rate
- Top up before running dry

### How do participants claim rewards?

Participants claim rewards using:

```bash
psyche-solana-client treasurer-claim-rewards \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --wallet-private-key-path [THEIR_WALLET]
```

**Claim process:**

- Converts earned points to tokens
- Transfers from treasury to participant wallet
- Points are consumed upon claim (can't claim twice)

**As run owner, you:**

- Don't need to manually distribute rewards
- Just ensure treasury is funded
- Participants claim themselves when ready

**Treasury considerations:**

- If treasury runs dry, claims will fail
- Participants can claim anytime (not just at epoch end)
- No time limit on claims (as long as treasury has funds)

### Can I change reward rates mid-run?

**Yes, using `set-future-epoch-rates`:**

```bash
psyche-solana-client set-future-epoch-rates \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --earning-rate [NEW_EARNING_RATE] \
    --slashing-rate [NEW_SLASHING_RATE] \
    --wallet-private-key-path [WALLET_PATH]
```

**Important:**

- Changes apply to **future epochs only**
- Current/ongoing epoch uses old rates
- Allows gradual rate adjustments

**Use cases:**

- Increase rates to attract more participants
- Decrease rates to extend treasury lifespan
- Adjust slashing to penalize/encourage behavior

**Example scenario:**

```
Epoch 1-10: earning_rate=100
Epoch 11+: change to earning_rate=150 (to attract more clients)
Treasury must be funded accordingly
```

**Points already earned:**

- Not affected by rate changes
- Can still be claimed at original value
- Only new epochs use new rates

## Troubleshooting

### Why isn't my run progressing past WaitingForMembers?

**Common causes:**

1. **Not enough clients joined:**
   - Check `init_min_clients` in config
   - Wait for more clients to join
   - Verify clients are authorized (if permissioned)

2. **Run is still paused:**

   ```bash
   psyche-solana-client set-paused --resume ...
   ```

3. **Clients not authorized:**
   - Check authorization setup
   - Verify clients using correct AUTHORIZER

4. **Configuration errors:**
   - Review config with `json-dump-run`
   - Check for invalid parameters

**Diagnosis:**

```bash
psyche-solana-client json-dump-run --rpc [RPC] --run-id [RUN_ID]
```

Look for:

- `state: "WaitingForMembers"`
- Number of pending_clients vs init_min_clients
- Whether paused flag is set

### Clients are being ejected - what's wrong?

**Possible causes:**

1. **Clients too slow:**
   - Increase `max_round_train_time`
   - Clients need faster GPUs
   - Or reduce model size/batch size

2. **Network issues:**
   - Clients have poor P2P connectivity
   - Firewall blocking peer connections
   - High latency/packet loss

3. **Health check failures:**
   - Clients crashing/restarting
   - Not sending health checks
   - Not submitting training results

4. **Client software bugs:**
   - Outdated client version
   - Incompatible configuration
   - Report to Psyche team

**Investigation:**

```bash
# Check specific client status
psyche-solana-client json-dump-user \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --wallet [CLIENT_PUBKEY]

# Contact client owner for logs
# Ask them to run: docker logs psyche-client
```

**Solutions:**

- Adjust timing parameters to be more lenient
- Help clients troubleshoot connectivity
- Ensure clients meet hardware requirements

### How do I debug a stuck or failed run?

**Step 1: Check coordinator state:**

```bash
psyche-solana-client json-dump-run --rpc [RPC] --run-id [RUN_ID]
```

**Step 2: Identify the issue:**

- **Stuck in one state:** Check timing parameters, may need manual tick
- **No clients:** Authorization or client connection issues
- **Frequent epoch failures:** Witness quorum not reached, need more witnesses
- **State transitions erratic:** Possible RPC or network issues

**Step 3: Check on-chain events:**

Use Solana block explorer or RPC to see recent transactions on the run account.

**Step 4: Get client feedback:**

Contact participants:

- Are they seeing errors?
- Are they stuck in one phase?
- What do their logs show?

**Step 5: Restart strategies:**

- **Pause and resume:** May clear stuck states
- **Wait for epoch timeout:** Often self-resolves
- **Update config:** Fix problematic parameters
- **Increase min_clients:** If too many dropouts

**Emergency measures:**

- Create new run with lessons learned
- Migrate clients to new run
- Salvage checkpoints if possible

## See Also

- [Client FAQ](../joining-a-run/faq.md) - Questions from client perspective
- [Configuration In Depth](./configuration.md) - All config parameters explained
- [Authentication](./authentication.md) - Authorization management details
- [Rewards](../explain/rewards.md) - How the reward system works
- [Workflow Overview](../explain/workflow-overview.md) - Understanding coordinator states
