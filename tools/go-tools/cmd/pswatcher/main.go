package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"regexp"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/gorilla/websocket"
	"golang.org/x/crypto/ssh"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true },
}

type ServerConfig struct {
	Host             string      `json:"host"`
	ClientCount      int         `json:"clientCount"`
	WorkingDirectory string      `json:"workingDirectory"`
	MetricsStartPort int         `json:"metricsStartPort"`
	SSHClient        *ssh.Client `json:"-"`
}

type CommandConfig struct {
	Command string   `json:"command"`
	Args    []string `json:"args"`
}

type Config struct {
	Port            int                      `json:"port"`
	Verbose         bool                     `json:"verbose"`
	CoordinatorHost string                   `json:"coordinatorHost"`
	CheckInterval   string                   `json:"checkInterval"`
	Servers         []ServerConfig           `json:"servers"`
	Commands        map[string]CommandConfig `json:"commands"`
}

var clients = make(map[*websocket.Conn]bool)
var coordinatorReady = make(chan bool)
var processStates = make(map[string]map[string]any)
var runningProcesses = make(map[string]*exec.Cmd)
var runningSessions = make(map[string]*ssh.Session)
var metricsStopChannels = make(map[string]chan bool)
var processMutex sync.RWMutex
var clientsMutex sync.RWMutex
var connMutexes = make(map[*websocket.Conn]*sync.Mutex)
var clientPorts = make(map[string]int)
var config Config
var totalClientCount int
var checkInterval time.Duration

// Regular expression to match ANSI escape codes
var ansiRegex = regexp.MustCompile(`\x1b\[[0-9;]*m`)

func loadConfig(filename string) (Config, error) {
	var cfg Config

	// Set defaults
	cfg.Port = 8888
	cfg.Verbose = false
	cfg.CoordinatorHost = "localhost"
	cfg.CheckInterval = "5s"
	cfg.Servers = []ServerConfig{
		{
			Host:             "localhost",
			ClientCount:      1,
			WorkingDirectory: "~/src/psyche",
			MetricsStartPort: 9001,
		},
	}
	cfg.Commands = map[string]CommandConfig{
		"coordinator": {Command: "just", Args: []string{"setup-solana-localnet-dummy-test-run"}},
		"client":      {Command: "just", Args: []string{"start-training-localnet-light-client"}},
		"cleanup":     {Command: "killall", Args: []string{"solana-test-validator"}},
	}

	// If config file doesn't exist, return defaults
	if _, err := os.Stat(filename); os.IsNotExist(err) {
		return cfg, nil
	}

	// Load config from file
	file, err := os.Open(filename)
	if err != nil {
		return cfg, err
	}
	defer file.Close()

	decoder := json.NewDecoder(file)
	err = decoder.Decode(&cfg)
	if err != nil {
		return cfg, err
	}

	return cfg, nil
}

func stripAnsiCodes(text string) string {
	return ansiRegex.ReplaceAllString(text, "")
}

func stopMetricsChecker(name string) {
	if stopChan, exists := metricsStopChannels[name]; exists {
		select {
		case stopChan <- true:
			fmt.Printf("Sent stop signal to metrics checker for %s\n", name)
		default:
			// Channel already has a signal or is closed
		}
		delete(metricsStopChannels, name)
	}
}

func handleProcessOutput(name string, output io.Reader, isStderr bool, isCoordinator bool) {
	scanner := bufio.NewScanner(output)
	for scanner.Scan() {
		line := scanner.Text()

		// Strip ANSI escape codes for clean web display
		cleanLine := stripAnsiCodes(line)

		// Print original line with colors to console only if verbose mode is enabled
		if config.Verbose {
			fmt.Printf("[%s] %s\n", name, line)
		}
		updateProcessState(name, "output", "", cleanLine)

		// Properly encode JSON to avoid control character issues
		messageData := map[string]any{
			"type":   "output",
			"name":   name,
			"output": cleanLine,
		}
		messageJSON, err := json.Marshal(messageData)
		if err != nil {
			fmt.Printf("[ERROR] Failed to marshal JSON for %s: %v\n", name, err)
			return
		}

		broadcast(string(messageJSON))

		if isCoordinator && !isStderr {
			if strings.Contains(cleanLine, "Streaming transaction logs. Confirmed commitment") {
				fmt.Printf("[COORDINATOR] Detected ready signal: %s\n", cleanLine)
				select {
				case coordinatorReady <- true:
					fmt.Printf("[COORDINATOR] Sent ready signal to channel\n")
				default:
					fmt.Printf("[COORDINATOR] Ready signal channel already notified\n")
				}
			}

			if strings.Contains(cleanLine, "Program log: Post-tick run state: ") {
				stateStart := strings.Index(cleanLine, "Program log: Post-tick run state: ") + len("Program log: Post-tick run state: ")
				if stateStart < len(cleanLine) {
					state := cleanLine[stateStart:]
					updateProcessState(name, "state", state, nil)
					broadcastState(name, state)
				}
			}
		}
	}
}

func broadcast(message string) {
	clientsMutex.RLock()
	clientsCopy := make(map[*websocket.Conn]*sync.Mutex)
	for client := range clients {
		clientsCopy[client] = connMutexes[client]
	}
	clientsMutex.RUnlock()

	for client, mutex := range clientsCopy {
		go func(c *websocket.Conn, m *sync.Mutex) {
			m.Lock()
			defer m.Unlock()
			c.WriteMessage(websocket.TextMessage, []byte(message))
		}(client, mutex)
	}
}

func broadcastMessage(msgType, name, status string) {
	messageData := map[string]any{
		"type":   msgType,
		"name":   name,
		"status": status,
	}
	messageJSON, err := json.Marshal(messageData)
	if err != nil {
		fmt.Printf("[ERROR] Failed to marshal JSON for %s: %v\n", name, err)
		return
	}
	broadcast(string(messageJSON))
}

func broadcastState(name, state string) {
	messageData := map[string]any{
		"type":  "state",
		"name":  name,
		"state": state,
	}
	messageJSON, err := json.Marshal(messageData)
	if err != nil {
		fmt.Printf("[ERROR] Failed to marshal JSON for %s: %v\n", name, err)
		return
	}
	broadcast(string(messageJSON))
}

func updateProcessState(name, msgType, status string, data any) {
	processMutex.Lock()
	defer processMutex.Unlock()

	if processStates[name] == nil {
		processStates[name] = make(map[string]any)
		processStates[name]["name"] = name
		processStates[name]["status"] = "starting"
		processStates[name]["output"] = []string{}
		processStates[name]["state"] = ""
		processStates[name]["metrics"] = nil
	}

	if msgType == "status" {
		processStates[name]["status"] = status
	} else if msgType == "output" {
		outputList := processStates[name]["output"].([]string)
		outputList = append(outputList, data.(string))
		if len(outputList) > 500 {
			outputList = outputList[1:]
		}
		processStates[name]["output"] = outputList
	} else if msgType == "state" {
		processStates[name]["state"] = status
	} else if msgType == "metrics" {
		processStates[name]["metrics"] = data
	}
}

func getMetricsHost(serverHost string) string {
	if serverHost == "" {
		return "localhost"
	}
	if strings.Contains(serverHost, "@") {
		parts := strings.SplitN(serverHost, "@", 2)
		return parts[1]
	}
	return serverHost
}

func checkMetricsPort(name, serverHost string, port int) {
	host := getMetricsHost(serverHost)
	fmt.Printf("[%s] Starting metrics checker for %s:%d\n", name, host, port)

	stopChan := make(chan bool, 1)
	processMutex.Lock()
	metricsStopChannels[name] = stopChan
	processMutex.Unlock()

	for {
		select {
		case <-stopChan:
			fmt.Printf("[%s] Stopping metrics checker\n", name)
			return
		default:
		}

		if !checkAndProcessMetrics(name, host, port, stopChan) {
			return
		}
	}
}

func checkAndProcessMetrics(name, host string, port int, stopChan chan bool) bool {
	conn, err := net.DialTimeout("tcp", fmt.Sprintf("%s:%d", host, port), 2*time.Second)
	if err != nil {
		return waitOrStop(name, stopChan, checkInterval)
	}
	defer conn.Close()

	buffer := make([]byte, 4096)
	n, err := conn.Read(buffer)
	if err != nil && err != io.EOF {
		return waitOrStop(name, stopChan, checkInterval)
	}

	if n > 0 {
		response := string(buffer[:n])
		var metrics map[string]any
		if err := json.Unmarshal([]byte(response), &metrics); err == nil {
			// Check if this is the first time we're getting metrics - if so, change status to "running"
			processMutex.RLock()
			currentStatus := ""
			if processStates[name] != nil {
				currentStatus = processStates[name]["status"].(string)
			}
			processMutex.RUnlock()
			
			if currentStatus == "starting" {
				updateProcessState(name, "status", "running", nil)
				broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"running"}`, name))
			}
			
			updateProcessState(name, "metrics", "", metrics)
			metricsJSON, _ := json.Marshal(map[string]any{
				"type": "metrics", "name": name, "metrics": metrics,
			})
			broadcast(string(metricsJSON))
		}
	}

	return waitOrStop(name, stopChan, checkInterval)
}

func waitOrStop(name string, stopChan chan bool, duration time.Duration) bool {
	select {
	case <-stopChan:
		fmt.Printf("[%s] Stopping metrics checker\n", name)
		return false
	case <-time.After(duration):
		return true
	}
}

func getServerName(host string) string {
	if host == "" {
		return "local"
	}
	if strings.Contains(host, "@") {
		parts := strings.SplitN(host, "@", 2)
		return parts[1]
	}
	return host
}

func handleWebSocketCommand(message []byte) {
	var cmd map[string]any
	if err := json.Unmarshal(message, &cmd); err != nil {
		fmt.Printf("Error parsing WebSocket message: %v\n", err)
		return
	}

	fmt.Printf("Received WebSocket command: %+v\n", cmd)

	switch cmd["type"] {
	case "kill":
		if processName, ok := cmd["name"].(string); ok {
			fmt.Printf("Attempting to kill process: %s\n", processName)
			killProcess(processName)
		}
	case "kill_all":
		fmt.Printf("Attempting to kill all processes\n")
		killAllProcesses()
	}
}

func connectSSH(serverHost string) (*ssh.Client, error) {
	// Parse user@host format
	user := os.Getenv("USER")
	host := serverHost
	if strings.Contains(serverHost, "@") {
		parts := strings.SplitN(serverHost, "@", 2)
		user = parts[0]
		host = parts[1]
	}

	config := &ssh.ClientConfig{
		User: user,
		Auth: []ssh.AuthMethod{
			ssh.PublicKeysCallback(func() ([]ssh.Signer, error) {
				homeDir := os.Getenv("HOME")
				keyPath := homeDir + "/.ssh/id_rsa"
				if _, err := os.Stat(keyPath); os.IsNotExist(err) {
					keyPath = homeDir + "/.ssh/id_ed25519"
				}
				key, err := os.ReadFile(keyPath)
				if err != nil {
					return nil, err
				}
				signer, err := ssh.ParsePrivateKey(key)
				if err != nil {
					return nil, err
				}
				return []ssh.Signer{signer}, nil
			}),
		},
		HostKeyCallback: ssh.InsecureIgnoreHostKey(),
		Timeout:         30 * time.Second,
	}

	client, err := ssh.Dial("tcp", host+":22", config)
	if err != nil {
		return nil, err
	}

	return client, nil
}

func runProcess(name, command string, args []string, isCoordinator bool, env []string, sshClient *ssh.Client, serverHost string, workingDir string) {
	if sshClient != nil {
		runSSHProcess(name, command, args, isCoordinator, env, sshClient, serverHost, workingDir)
		return
	}

	cmd := exec.Command(command, args...)
	// Expand ~ to home directory
	if strings.HasPrefix(workingDir, "~/") {
		workingDir = os.Getenv("HOME") + workingDir[1:]
	}
	cmd.Dir = workingDir
	cmd.Env = os.Environ()
	if env != nil {
		cmd.Env = append(cmd.Env, env...)
	}

	// Set up process group so we can kill all child processes
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}

	stdout, _ := cmd.StdoutPipe()
	stderr, _ := cmd.StderrPipe()

	processMutex.Lock()
	runningProcesses[name] = cmd
	processMutex.Unlock()

	updateProcessState(name, "status", "starting", nil)
	broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"starting"}`, name))

	fmt.Printf("[%s] Starting process: %s %v\n", name, command, args)
	if err := cmd.Start(); err != nil {
		fmt.Printf("[%s] Failed to start: %v\n", name, err)
		updateProcessState(name, "status", "failed", nil)
		broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"failed"}`, name))
		return
	}

	go handleProcessOutput(name, stdout, false, isCoordinator)

	go handleProcessOutput(name, stderr, true, isCoordinator)

	go func() {
		cmd.Wait()
		fmt.Printf("[%s] Process finished\n", name)
		processMutex.Lock()
		delete(runningProcesses, name)
		processMutex.Unlock()
		updateProcessState(name, "status", "finished", nil)
		broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"finished"}`, name))
	}()
}

func runSSHProcess(name, command string, args []string, isCoordinator bool, env []string, sshClient *ssh.Client, serverHost string, workingDir string) {
	updateProcessState(name, "status", "starting", nil)
	broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"starting"}`, name))

	// Build the command string
	cmdStr := command
	if len(args) > 0 {
		cmdStr += " " + strings.Join(args, " ")
	}

	// Add environment variables
	if env != nil {
		envStr := strings.Join(env, " ")
		cmdStr = envStr + " " + cmdStr
	}

	// Use the working directory from config
	workDir := workingDir

	// Extract username from serverHost for path construction
	username := "admin" // default fallback
	if strings.Contains(serverHost, "@") {
		parts := strings.SplitN(serverHost, "@", 2)
		username = parts[0]
	}

	// Change to the working directory and run the command with proper environment
	fullCmd := fmt.Sprintf("export PATH=\"/home/%s/.local/share/solana/install/active_release/bin:/usr/local/cuda/bin:/home/%s/.local/bin:/home/%s/bin:/home/%s/.nvm/versions/node/v22.17.0/bin:/home/%s/.cargo/bin:$PATH\" && export LIBTORCH=$HOME/bin/libtorch && export LIBTORCH_INCLUDE=$LIBTORCH && export LIBTORCH_LIB=$LIBTORCH && export LD_LIBRARY_PATH=$LIBTORCH/lib:$LD_LIBRARY_PATH && cd %s && %s", username, username, username, username, username, workDir, cmdStr)

	fmt.Printf("[%s] Starting SSH process: %s\n", name, fullCmd)

	session, err := sshClient.NewSession()
	if err != nil {
		fmt.Printf("[%s] Failed to create SSH session: %v\n", name, err)
		updateProcessState(name, "status", "failed", nil)
		broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"failed"}`, name))
		return
	}

	stdout, err := session.StdoutPipe()
	if err != nil {
		fmt.Printf("[%s] Failed to get stdout pipe: %v\n", name, err)
		session.Close()
		updateProcessState(name, "status", "failed", nil)
		broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"failed"}`, name))
		return
	}

	stderr, err := session.StderrPipe()
	if err != nil {
		fmt.Printf("[%s] Failed to get stderr pipe: %v\n", name, err)
		session.Close()
		updateProcessState(name, "status", "failed", nil)
		broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"failed"}`, name))
		return
	}

	// Store the session for later cleanup
	processMutex.Lock()
	runningSessions[name] = session
	processMutex.Unlock()

	// Start the command
	if err := session.Start(fullCmd); err != nil {
		fmt.Printf("[%s] Failed to start SSH command: %v\n", name, err)
		session.Close()
		updateProcessState(name, "status", "failed", nil)
		broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"failed"}`, name))
		return
	}

	// Handle stdout
	go handleProcessOutput(name, stdout, false, isCoordinator)

	// Handle stderr
	go handleProcessOutput(name, stderr, true, isCoordinator)

	// Wait for the command to complete
	go func() {
		session.Wait()
		session.Close()
		fmt.Printf("[%s] SSH process finished\n", name)
		processMutex.Lock()
		delete(runningSessions, name)
		processMutex.Unlock()
		updateProcessState(name, "status", "finished", nil)
		broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"finished"}`, name))
	}()
}

func handleWebSocket(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		return
	}
	defer conn.Close()

	clientsMutex.Lock()
	clients[conn] = true
	connMutexes[conn] = &sync.Mutex{}
	clientsMutex.Unlock()

	defer func() {
		clientsMutex.Lock()
		delete(clients, conn)
		delete(connMutexes, conn)
		clientsMutex.Unlock()
	}()

	connMutex := connMutexes[conn]

	// Send initial configuration with client count and server information
	configJSON, _ := json.Marshal(map[string]any{
		"type":        "config",
		"clientCount": totalClientCount,
		"servers":     config.Servers,
	})
	connMutex.Lock()
	conn.WriteMessage(websocket.TextMessage, configJSON)
	connMutex.Unlock()

	// Send existing process states
	processMutex.RLock()
	statesCopy := make(map[string]map[string]any)
	for name, state := range processStates {
		statesCopy[name] = make(map[string]any)
		for k, v := range state {
			statesCopy[name][k] = v
		}
	}
	processMutex.RUnlock()

	for name, state := range statesCopy {
		stateJSON, _ := json.Marshal(map[string]any{
			"type":    "status",
			"name":    name,
			"status":  state["status"],
			"output":  state["output"],
			"state":   state["state"],
			"metrics": state["metrics"],
		})
		connMutex.Lock()
		conn.WriteMessage(websocket.TextMessage, stateJSON)
		connMutex.Unlock()
	}

	for {
		_, message, err := conn.ReadMessage()
		if err != nil {
			break
		}

		handleWebSocketCommand(message)
	}
}

func killLocalProcess(name string, cmd *exec.Cmd) {
	fmt.Printf("Killing local process group for: %s (PID: %d)\n", name, cmd.Process.Pid)

	// Kill the entire process group
	pgid, err := syscall.Getpgid(cmd.Process.Pid)
	if err != nil {
		fmt.Printf("Error getting process group ID for %s: %v\n", name, err)
		// Fallback to killing just the parent process
		if err := cmd.Process.Kill(); err != nil {
			fmt.Printf("Error killing process %s: %v\n", name, err)
		}
	} else {
		fmt.Printf("Process group ID for %s: %d\n", name, pgid)
		// Kill the entire process group
		if err := syscall.Kill(-pgid, syscall.SIGTERM); err != nil {
			fmt.Printf("Error killing process group %d: %v\n", pgid, err)
			// Fallback to SIGKILL if SIGTERM fails
			if err := syscall.Kill(-pgid, syscall.SIGKILL); err != nil {
				fmt.Printf("Error force killing process group %d: %v\n", pgid, err)
			}
		} else {
			fmt.Printf("Successfully sent SIGTERM to process group %d\n", pgid)
		}
	}

	updateProcessState(name, "status", "finalized", nil)
	broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"finalized"}`, name))
}

func killSSHSession(name string, session *ssh.Session) {
	fmt.Printf("Killing SSH session for: %s\n", name)

	// Send interrupt signal to the remote process
	if err := session.Signal(ssh.SIGTERM); err != nil {
		fmt.Printf("Error sending SIGTERM to SSH session %s: %v\n", name, err)
		// Fallback to closing the session
		session.Close()
	} else {
		fmt.Printf("Successfully sent SIGTERM to SSH session %s\n", name)
	}

	updateProcessState(name, "status", "finalized", nil)
	broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"finalized"}`, name))
}

func killProcess(name string) {
	// First update status to "killing"
	updateProcessState(name, "status", "killing", nil)
	broadcast(fmt.Sprintf(`{"type":"status","name":"%s","status":"killing"}`, name))

	processMutex.Lock()
	defer processMutex.Unlock()

	// Check if it's a local process
	if cmd, exists := runningProcesses[name]; exists {
		killLocalProcess(name, cmd)
		delete(runningProcesses, name)
		stopMetricsChecker(name)
		return
	}

	// Check if it's an SSH session
	if session, exists := runningSessions[name]; exists {
		killSSHSession(name, session)
		delete(runningSessions, name)
		stopMetricsChecker(name)
		return
	}

	fmt.Printf("Process %s not found in running processes or sessions\n", name)
}

func killAllProcesses() {
	processMutex.Lock()
	defer processMutex.Unlock()

	fmt.Println("Killing all processes...")

	// Kill local processes
	for name, cmd := range runningProcesses {
		killLocalProcess(name, cmd)
	}

	// Kill SSH sessions
	for name, session := range runningSessions {
		killSSHSession(name, session)
	}

	// Stop all metrics checkers
	for name, stopChan := range metricsStopChannels {
		select {
		case stopChan <- true:
			fmt.Printf("Sent stop signal to metrics checker for %s\n", name)
		default:
			// Channel already has a signal or is closed
		}
	}

	// Clear the maps
	runningProcesses = make(map[string]*exec.Cmd)
	runningSessions = make(map[string]*ssh.Session)
	metricsStopChannels = make(map[string]chan bool)

	// Run fallback killall commands on all servers
	for _, server := range config.Servers {
		if server.SSHClient != nil {
			fmt.Printf("Running fallback cleanup commands on remote server %s...\n", server.Host)

			session, err := server.SSHClient.NewSession()
			if err != nil {
				fmt.Printf("Failed to create SSH session for cleanup on %s: %v\n", server.Host, err)
			} else {
				cleanupCmd := config.Commands["cleanup"]
				killallCmd := fmt.Sprintf("%s %s", cleanupCmd.Command, strings.Join(cleanupCmd.Args, " "))
				fmt.Printf("Executing cleanup command on %s: %s\n", server.Host, killallCmd)

				if err := session.Run(killallCmd); err != nil {
					fmt.Printf("Cleanup command completed on %s (some processes may not have existed): %v\n", server.Host, err)
				} else {
					fmt.Printf("Cleanup command completed successfully on %s\n", server.Host)
				}
				session.Close()
			}
		} else if server.Host == "" || server.Host == "localhost" {
			fmt.Printf("Running fallback cleanup commands on localhost...\n")
			
			cleanupCmd := config.Commands["cleanup"]
			fmt.Printf("Executing cleanup command locally: %s %s\n", cleanupCmd.Command, strings.Join(cleanupCmd.Args, " "))
			
			cmd := exec.Command(cleanupCmd.Command, cleanupCmd.Args...)
			if err := cmd.Run(); err != nil {
				fmt.Printf("Local cleanup command completed (some processes may not have existed): %v\n", err)
			} else {
				fmt.Printf("Local cleanup command completed successfully\n")
			}
		}
	}
}

func initConfig() error {
	configPath := "./config.json"

	// First CLI argument is config file path
	if len(os.Args) > 1 {
		configPath = os.Args[1]
	}

	var err error
	config, err = loadConfig(configPath)
	if err != nil {
		return fmt.Errorf("failed to load config from %s: %v", configPath, err)
	}

	// Parse and cache check interval
	checkInterval, err = time.ParseDuration(config.CheckInterval)
	if err != nil {
		fmt.Printf("Error parsing checkInterval '%s', using default 5s: %v\n", config.CheckInterval, err)
		checkInterval = 5 * time.Second
	}

	// Calculate total client count
	totalClientCount = 0
	for _, server := range config.Servers {
		totalClientCount += server.ClientCount
	}

	fmt.Printf("Loaded configuration from: %s\n", configPath)
	return nil
}

func main() {
	if err := initConfig(); err != nil {
		log.Fatalf("Configuration error: %v", err)
	}

	// Connect to all SSH servers
	for i := range config.Servers {
		if config.Servers[i].Host != "" && config.Servers[i].Host != "localhost" {
			client, err := connectSSH(config.Servers[i].Host)
			if err != nil {
				log.Fatalf("Failed to connect to SSH server %s: %v", config.Servers[i].Host, err)
			}
			config.Servers[i].SSHClient = client
			fmt.Printf("Connected to SSH server: %s\n", config.Servers[i].Host)
		}
	}

	// Cleanup SSH connections on exit
	defer func() {
		for _, server := range config.Servers {
			if server.SSHClient != nil {
				server.SSHClient.Close()
			}
		}
	}()

	http.HandleFunc("/ws", handleWebSocket)
	http.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		http.ServeFile(w, r, "index.html")
	})

	go func() {
		fmt.Println("Killing any existing processes...")

		// Kill existing processes on all servers
		cleanupCmd := config.Commands["cleanup"]
		for _, server := range config.Servers {
			if server.Host == "" || server.Host == "localhost" {
				// Local execution
				cmdStr := fmt.Sprintf("%s %s", cleanupCmd.Command, strings.Join(cleanupCmd.Args, " "))
				fmt.Printf("Executing cleanup command locally: %s\n", cmdStr)
				killCmd := exec.Command(cleanupCmd.Command, cleanupCmd.Args...)
				if err := killCmd.Run(); err != nil {
					fmt.Printf("Local cleanup command completed (some processes may not have existed): %v\n", err)
				} else {
					fmt.Printf("Local cleanup command completed successfully\n")
				}
			}
		}

		// Find coordinator server
		var coordServer *ServerConfig
		for i := range config.Servers {
			if config.Servers[i].Host == config.CoordinatorHost {
				coordServer = &config.Servers[i]
				break
			}
		}
		if coordServer == nil {
			log.Fatalf("Coordinator host %s not found in servers list", config.CoordinatorHost)
		}

		// Start coordinator
		fmt.Printf("[MAIN] Starting coordinator on server: %s\n", coordServer.Host)
		coordCmd := config.Commands["coordinator"]
		runProcess("coordinator", coordCmd.Command, coordCmd.Args, true, nil, coordServer.SSHClient, coordServer.Host, coordServer.WorkingDirectory)

		fmt.Printf("[MAIN] Waiting for coordinator to be ready...\n")
		select {
		case <-coordinatorReady:
			fmt.Printf("[MAIN] Coordinator is ready! Starting clients...\n")
		case <-time.After(300 * time.Second): // 5 minute timeout
			fmt.Printf("[MAIN] WARNING: Coordinator ready timeout after 5 minutes, starting clients anyway...\n")
		}
		updateProcessState("coordinator", "status", "ready", nil)
		broadcastMessage("status", "coordinator", "ready")

		// Start clients across all servers (after coordinator is ready)
		clientIndex := 1
		coordinatorHost := getMetricsHost(coordServer.Host) // Get the coordinator's host

		for _, server := range config.Servers {
			port := server.MetricsStartPort
			for i := 0; i < server.ClientCount; i++ {
				clientName := fmt.Sprintf("%s-client%d", getServerName(server.Host), clientIndex)

				// Set up environment variables
				env := []string{fmt.Sprintf("METRICS_LOCAL_BIND=0.0.0.0:%d", port)}

				// If this client is on a different server than the coordinator, set RPC endpoints
				if server.Host != coordServer.Host {
					env = append(env, fmt.Sprintf("RPC=http://%s:8899", coordinatorHost))
					env = append(env, fmt.Sprintf("WS_RPC=ws://%s:8900", coordinatorHost))
					fmt.Printf("[CLIENT] Setting RPC endpoints for %s: RPC=http://%s:8899, WS_RPC=ws://%s:8900\n", clientName, coordinatorHost, coordinatorHost)
				}

				clientPorts[clientName] = port

				// Start client process
				clientCmd := config.Commands["client"]
				runProcess(clientName, clientCmd.Command, clientCmd.Args, false, env, server.SSHClient, server.Host, server.WorkingDirectory)
				go checkMetricsPort(clientName, server.Host, port)

				clientIndex++
				port++
			}
		}
	}()

	c := make(chan os.Signal, 1)
	signal.Notify(c, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-c
		fmt.Println("\nReceived shutdown signal (Ctrl+C). Gracefully shutting down...")

		// Create a timeout channel for cleanup
		done := make(chan bool)
		go func() {
			// Use the comprehensive killAllProcesses function
			killAllProcesses()

			// Close SSH connections
			for _, server := range config.Servers {
				if server.SSHClient != nil {
					fmt.Printf("Closing SSH connection to %s...\n", server.Host)
					server.SSHClient.Close()
				}
			}
			done <- true
		}()

		// Wait for cleanup or timeout
		select {
		case <-done:
			fmt.Println("Graceful shutdown complete.")
		case <-time.After(15 * time.Second):
			fmt.Println("Cleanup timeout reached. Forcing shutdown.")
		}
		
		os.Exit(0)
	}()

	fmt.Printf("Server running on http://localhost:%d with %d total clients across %d servers\n", config.Port, totalClientCount, len(config.Servers))
	for _, server := range config.Servers {
		if server.Host == "" || server.Host == "localhost" {
			fmt.Printf("  Local: %d clients\n", server.ClientCount)
		} else {
			fmt.Printf("  %s: %d clients\n", server.Host, server.ClientCount)
		}
	}
	
	server := &http.Server{Addr: fmt.Sprintf(":%d", config.Port)}
	go func() {
		if err := server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatal(err)
		}
	}()
	
	// Wait for shutdown signal
	select {}
}
