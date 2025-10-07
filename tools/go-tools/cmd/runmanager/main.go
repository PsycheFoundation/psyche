package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"strings"
	"syscall"

	"github.com/gagliardetto/solana-go"
)

type envVarsFlag []string

func (e *envVarsFlag) String() string {
	return strings.Join(*e, ", ")
}

func (e *envVarsFlag) Set(value string) error {
	*e = append(*e, value)
	return nil
}

func main() {
	runID := flag.String("run-id", "", "ID of the run to join")
	rpc := flag.String("rpc", "", "Solana RPC endpoint (http://localhost:8899)")
	wsRpc := flag.String("ws-rpc", "", "Solana WebSocket RPC endpoint (ws://localhost:8900)")
	walletPath := flag.String("wallet-path", "", "Path to wallet private key file")
	var envVars envVarsFlag
	flag.Var(&envVars, "env", "Environment variable in KEY=VALUE format (--env ENV_1=val --env ENV_2=val ... etc)")
	background := flag.Bool("background", false, "Run container in background")

	flag.Parse()

	if *runID == "" {
		fmt.Fprintf(os.Stderr, "Error: --run-id is required\n")
		flag.Usage()
		os.Exit(1)
	}

	if *rpc == "" {
		fmt.Fprintf(os.Stderr, "Error: --rpc is required\n")
		flag.Usage()
		os.Exit(1)
	}

	if *wsRpc == "" {
		fmt.Fprintf(os.Stderr, "Error: --ws-rpc is required\n")
		flag.Usage()
		os.Exit(1)
	}

	if *walletPath == "" {
		fmt.Fprintf(os.Stderr, "Error: --wallet-path is required\n")
		flag.Usage()
		os.Exit(1)
	}

	if err := run(*runID, *rpc, *wsRpc, *walletPath, envVars, *background); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func run(runID, rpc, wsRpc, walletPath string, envVars []string, background bool) error {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// graceful shutdown
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-sigChan
		fmt.Println("\nReceived interrupt signal, shutting down...")
		cancel()
	}()

	// Read wallet private key
	fmt.Printf("Reading wallet from: %s\n", walletPath)
	walletKey, err := os.ReadFile(walletPath)
	if err != nil {
		return fmt.Errorf("failed to read wallet file: %w", err)
	}
	// Trim any whitespace/newlines
	walletKeyStr := strings.TrimSpace(string(walletKey))

	// Query coordinator for docker tag
	fmt.Printf("Querying coordinator for Run ID: %s\n", runID)

	// Parse the coordinator program ID
	// This is the on-chain program ID from solana-coordinator/src/lib.rs
	coordinatorProgramID := solana.MustPublicKeyFromBase58("HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y")

	coordinator := NewCoordinatorClient(rpc, coordinatorProgramID)
	dockerTag, err := coordinator.GetDockerTagForRun(runID)
	if err != nil {
		return fmt.Errorf("failed to get docker tag for run: %w", err)
	}
	fmt.Printf("Docker tag for run '%s': %s\n", runID, dockerTag)

	// Get version for validation
	version, err := coordinator.GetRunVersion(runID)
	if err != nil {
		return fmt.Errorf("failed to extract version: %w", err)
	}
	fmt.Printf("Required version: %s\n", version)

	// Initialize Docker manager
	dockerMgr, err := NewDockerManager()
	if err != nil {
		return fmt.Errorf("failed to initialize Docker manager: %w", err)
	}
	defer dockerMgr.Close()

	// Always pull the image to ensure we have the correct version
	if err := dockerMgr.PullImage(ctx, dockerTag); err != nil {
		return fmt.Errorf("failed to pull image: %w", err)
	}

	allEnvVars := []string{
		// Required by train_entrypoint.sh
		fmt.Sprintf("RUN_ID=%s", runID),
		fmt.Sprintf("RPC=%s", rpc),
		fmt.Sprintf("WS_RPC=%s", wsRpc),
		fmt.Sprintf("RAW_WALLET_PRIVATE_KEY=%s", walletKeyStr),
		"NVIDIA_DRIVER_CAPABILITIES=compute,utility",
	}
	allEnvVars = append(allEnvVars, envVars...)

	// Run the container
	containerID, err := dockerMgr.RunContainer(ctx, dockerTag, allEnvVars)
	if err != nil {
		return fmt.Errorf("failed to run container: %w", err)
	}

	if background {
		fmt.Printf("\nContainer is running in the background.\n")
		fmt.Printf("To view logs: docker logs -f %s\n", containerID[:12])
		fmt.Printf("To stop: docker stop %s\n", containerID[:12])
	} else {
		if err := dockerMgr.StreamLogs(ctx, containerID); err != nil {
			return fmt.Errorf("failed to stream logs: %w", err)
		}
	}

	return nil
}
