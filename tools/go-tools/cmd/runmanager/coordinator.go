package main

import (
	"context"
	"fmt"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"
)

const (
	SOLANA_MAX_STRING_LEN = 64
	// CoordinatorInstance::SEEDS_PREFIX from the Solana program
	COORDINATOR_INSTANCE_PREFIX = "coordinator"
)

type CoordinatorClient struct {
	rpcClient *rpc.Client
	programID solana.PublicKey
}

func NewCoordinatorClient(rpcEndpoint string, programID solana.PublicKey) *CoordinatorClient {
	return &CoordinatorClient{
		rpcClient: rpc.New(rpcEndpoint),
		programID: programID,
	}
}

// Truncates a string to SOLANA_MAX_STRING_LEN bytes
func bytesFromString(s string) []byte {
	maxLen := SOLANA_MAX_STRING_LEN
	if len(s) < maxLen {
		maxLen = len(s)
	}
	return []byte(s[:maxLen])
}

// Derives the PDA for the coordinator instance account
func (c *CoordinatorClient) findCoordinatorInstance(runID string) (solana.PublicKey, uint8, error) {
	seeds := [][]byte{
		[]byte(COORDINATOR_INSTANCE_PREFIX),
		bytesFromString(runID),
	}
	return solana.FindProgramAddress(seeds, c.programID)
}

// CoordinatorInstanceData represents the parsed on-chain coordinator instance
type CoordinatorInstanceData struct {
	Bump               uint8
	MainAuthority      solana.PublicKey
	JoinAuthority      solana.PublicKey
	CoordinatorAccount solana.PublicKey
	RunID              string
}

// Fetches and parses the CoordinatorInstance from Solana
func (c *CoordinatorClient) fetchCoordinatorData(ctx context.Context, runID string) (*CoordinatorInstanceData, error) {
	// Derive the coordinator instance PDA
	coordinatorInstance, _, err := c.findCoordinatorInstance(runID)
	if err != nil {
		return nil, fmt.Errorf("failed to derive coordinator PDA: %w", err)
	}

	// Fetch the account data from Solana
	accountInfo, err := c.rpcClient.GetAccountInfo(ctx, coordinatorInstance)
	if err != nil {
		return nil, fmt.Errorf("RPC error: %w", err)
	}

	if accountInfo.Value == nil {
		return nil, fmt.Errorf("coordinator instance not found on-chain")
	}

	accountData := accountInfo.Value.Data.GetBinary()

	// - 8 bytes: Anchor discriminator
	// - 1 byte: bump
	// - 32 bytes: main_authority (Pubkey)
	// - 32 bytes: join_authority (Pubkey)
	// - 32 bytes: coordinator_account (Pubkey)
	// - 4 bytes: run_id length (u32 little-endian)
	// - N bytes: run_id string data
	const (
		DISCRIMINATOR_SIZE = 8
		BUMP_SIZE          = 1
		PUBKEY_SIZE        = 32
		RUN_ID_SIZE        = 4
	)

	expectedMinSize := DISCRIMINATOR_SIZE + BUMP_SIZE + (3 * PUBKEY_SIZE) + RUN_ID_SIZE
	if len(accountData) < expectedMinSize {
		return nil, fmt.Errorf("account data too short: got %d bytes, expected at least %d", len(accountData), expectedMinSize)
	}

	offset := DISCRIMINATOR_SIZE

	bumpByte := accountData[offset]
	offset += BUMP_SIZE

	mainAuthority := solana.PublicKeyFromBytes(accountData[offset : offset+PUBKEY_SIZE])
	offset += PUBKEY_SIZE

	joinAuthority := solana.PublicKeyFromBytes(accountData[offset : offset+PUBKEY_SIZE])
	offset += PUBKEY_SIZE

	coordinatorAccount := solana.PublicKeyFromBytes(accountData[offset : offset+PUBKEY_SIZE])
	offset += PUBKEY_SIZE

	// Parse run_id string
	runIDLen := uint32(accountData[offset]) |
		uint32(accountData[offset+1])<<8 |
		uint32(accountData[offset+2])<<16 |
		uint32(accountData[offset+3])<<24
	offset += RUN_ID_SIZE

	if runIDLen > SOLANA_MAX_STRING_LEN {
		return nil, fmt.Errorf("run_id length %d exceeds max %d", runIDLen, SOLANA_MAX_STRING_LEN)
	}

	if len(accountData) < offset+int(runIDLen) {
		return nil, fmt.Errorf("insufficient data for run_id string")
	}

	parsedRunID := string(accountData[offset : offset+int(runIDLen)])

	instance := &CoordinatorInstanceData{
		Bump:               bumpByte,
		MainAuthority:      mainAuthority,
		JoinAuthority:      joinAuthority,
		CoordinatorAccount: coordinatorAccount,
		RunID:              parsedRunID,
	}

	fmt.Printf("Fetched CoordinatorInstance from chain: { run_id: %s, coordinator_account: %s }\n",
		instance.RunID, instance.CoordinatorAccount.String())

	return instance, nil
}

// Queries the Solana coordinator to get the required docker image tag
func (c *CoordinatorClient) GetDockerTagForRun(runID string) (string, error) {
	ctx := context.Background()

	// Fetch coordinator instance from Solana
	_, err := c.fetchCoordinatorData(ctx, runID)
	if err != nil {
		return "", fmt.Errorf("failed to fetch coordinator from Solana: %w", err)
	}

	// TODO: When version field is added to CoordinatorInstance, use it here:
	// dockerTag := fmt.Sprintf("nousresearch/psyche-client:%s", instance.Version)
	// return dockerTag, nil

	dockerTag := fmt.Sprintf("nousresearch/psyche-client:%s", "latest")
	return dockerTag, nil
}

// Extracts the version from the docker tag "nousresearch/psyche-client:v1.2.3" -> "v1.2.3"
func (c *CoordinatorClient) GetRunVersion(runID string) (string, error) {
	dockerTag, err := c.GetDockerTagForRun(runID)
	if err != nil {
		return "", err
	}

	for i := len(dockerTag) - 1; i >= 0; i-- {
		if dockerTag[i] == ':' {
			return dockerTag[i+1:], nil
		}
	}

	return "", fmt.Errorf("invalid docker tag format: %s", dockerTag)
}
