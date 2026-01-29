#!/usr/bin/env bash
#
# Psyche Daemon Management Script
# Manages psyche-solana-client as a systemd user service
#
# Usage: psyche-daemon.sh <command> [run_id] [options]
#
# Commands:
#   install           Install systemd service files
#   start <run_id>    Start the training client for a run
#   stop <run_id>     Stop the training client
#   restart <run_id>  Restart the training client
#   status <run_id>   Show service status
#   logs <run_id>     Show and follow logs
#   env <run_id>      Create/edit environment file

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
PSYCHE_CONFIG_DIR="${HOME}/.config/psyche"
SERVICE_NAME="psyche-client"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_systemd() {
    if ! command -v systemctl &> /dev/null; then
        log_error "systemctl not found. This script requires systemd."
        exit 1
    fi

    # Check if systemd user session is available
    if ! systemctl --user status &> /dev/null; then
        log_error "systemd user session not available. Make sure you're logged in properly (not just su'd)."
        exit 1
    fi
}

cmd_install() {
    check_systemd

    log_info "Installing systemd service files..."

    # Create directories
    mkdir -p "${SYSTEMD_USER_DIR}"
    mkdir -p "${PSYCHE_CONFIG_DIR}"

    # Copy service file
    local service_src="${PROJECT_ROOT}/tools/systemd/${SERVICE_NAME}@.service"
    local service_dst="${SYSTEMD_USER_DIR}/${SERVICE_NAME}@.service"

    if [[ ! -f "${service_src}" ]]; then
        log_error "Service file not found: ${service_src}"
        exit 1
    fi

    # Update WorkingDirectory to point to actual project root
    sed "s|%h/src/psyche|${PROJECT_ROOT}|g" "${service_src}" > "${service_dst}"

    log_info "Installed service to ${service_dst}"

    # Reload systemd
    systemctl --user daemon-reload
    log_info "Reloaded systemd daemon"

    # Enable lingering so services run after logout
    if command -v loginctl &> /dev/null; then
        if ! loginctl show-user "$USER" 2>/dev/null | grep -q "Linger=yes"; then
            log_warn "Enabling lingering for user $USER (may require sudo)..."
            sudo loginctl enable-linger "$USER" 2>/dev/null || \
                log_warn "Could not enable lingering. Services may stop after logout."
        fi
    fi

    log_info "Installation complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Create an environment file:"
    echo "     $0 env <run_id>"
    echo "  2. Start the daemon:"
    echo "     $0 start <run_id>"
}

cmd_env() {
    local run_id="${1:-}"
    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 env <run_id>"
        exit 1
    fi

    local env_file="${PSYCHE_CONFIG_DIR}/client-${run_id}.env"

    if [[ ! -f "${env_file}" ]]; then
        log_info "Creating new environment file: ${env_file}"
        mkdir -p "${PSYCHE_CONFIG_DIR}"
        cat > "${env_file}" << 'EOF'
# Psyche Client Configuration
# Edit these values for your setup

# Solana RPC endpoints
RPC=http://127.0.0.1:8899
WS_RPC=ws://127.0.0.1:8900

# Wallet path (REQUIRED - set this to your wallet file)
WALLET_PRIVATE_KEY_PATH=

# Authorizer address (use 111...111 for permissionless)
AUTHORIZER=11111111111111111111111111111111

# Training parameters
DP=1
TP=1
BATCH_SIZE=1

# Extra arguments (optional)
EXTRA_ARGS=
EOF
        log_info "Created template. Please edit ${env_file} to configure."
        echo ""
        echo "Required: Set WALLET_PRIVATE_KEY_PATH to your wallet file path"
    else
        log_info "Environment file exists: ${env_file}"
    fi

    # Open in editor if available
    if [[ -n "${EDITOR:-}" ]]; then
        "${EDITOR}" "${env_file}"
    else
        echo "Edit with: nano ${env_file}"
    fi
}

cmd_start() {
    local run_id="${1:-}"
    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 start <run_id>"
        exit 1
    fi

    check_systemd

    local env_file="${PSYCHE_CONFIG_DIR}/client-${run_id}.env"
    if [[ ! -f "${env_file}" ]]; then
        log_error "Environment file not found: ${env_file}"
        log_info "Create one with: $0 env ${run_id}"
        exit 1
    fi

    # Validate wallet is set
    source "${env_file}"
    if [[ -z "${WALLET_PRIVATE_KEY_PATH:-}" ]]; then
        log_error "WALLET_PRIVATE_KEY_PATH not set in ${env_file}"
        exit 1
    fi
    if [[ ! -f "${WALLET_PRIVATE_KEY_PATH}" ]]; then
        log_error "Wallet file not found: ${WALLET_PRIVATE_KEY_PATH}"
        exit 1
    fi

    log_info "Starting ${SERVICE_NAME}@${run_id}..."
    systemctl --user start "${SERVICE_NAME}@${run_id}.service"

    sleep 1
    if systemctl --user is-active --quiet "${SERVICE_NAME}@${run_id}.service"; then
        log_info "Service started successfully!"
        echo ""
        echo "View logs with: $0 logs ${run_id}"
        echo "Check status with: $0 status ${run_id}"
    else
        log_error "Service failed to start. Check logs:"
        journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n 20 --no-pager
    fi
}

cmd_stop() {
    local run_id="${1:-}"
    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 stop <run_id>"
        exit 1
    fi

    check_systemd

    log_info "Stopping ${SERVICE_NAME}@${run_id}..."
    systemctl --user stop "${SERVICE_NAME}@${run_id}.service"
    log_info "Service stopped."
}

cmd_restart() {
    local run_id="${1:-}"
    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 restart <run_id>"
        exit 1
    fi

    check_systemd

    log_info "Restarting ${SERVICE_NAME}@${run_id}..."
    systemctl --user restart "${SERVICE_NAME}@${run_id}.service"

    sleep 1
    if systemctl --user is-active --quiet "${SERVICE_NAME}@${run_id}.service"; then
        log_info "Service restarted successfully!"
    else
        log_error "Service failed to restart. Check logs:"
        journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n 20 --no-pager
    fi
}

cmd_status() {
    local run_id="${1:-}"
    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 status <run_id>"
        exit 1
    fi

    check_systemd

    systemctl --user status "${SERVICE_NAME}@${run_id}.service" --no-pager || true
}

cmd_logs() {
    local run_id="${1:-}"
    local lines="${2:-50}"

    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 logs <run_id> [num_lines]"
        exit 1
    fi

    check_systemd

    log_info "Showing logs for ${SERVICE_NAME}@${run_id} (Ctrl+C to exit)..."
    journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n "${lines}" -f
}

cmd_help() {
    cat << EOF
Psyche Daemon Management Script

Usage: $0 <command> [run_id] [options]

Commands:
  install           Install systemd service files (run once)
  env <run_id>      Create/edit environment file for a run
  start <run_id>    Start the training client
  stop <run_id>     Stop the training client
  restart <run_id>  Restart the training client
  status <run_id>   Show service status
  logs <run_id>     Show and follow logs (Ctrl+C to exit)
  help              Show this help message

Examples:
  $0 install                    # Install systemd services
  $0 env test                   # Create config for run "test"
  $0 start test                 # Start client for run "test"
  $0 logs test                  # Follow logs
  $0 stop test                  # Stop the client

Environment files are stored in: ${PSYCHE_CONFIG_DIR}/
Logs are managed by journald and can be viewed with 'journalctl --user'
EOF
}

# Main command dispatch
main() {
    local cmd="${1:-help}"
    shift || true

    case "${cmd}" in
        install)
            cmd_install "$@"
            ;;
        env)
            cmd_env "$@"
            ;;
        start)
            cmd_start "$@"
            ;;
        stop)
            cmd_stop "$@"
            ;;
        restart)
            cmd_restart "$@"
            ;;
        status)
            cmd_status "$@"
            ;;
        logs)
            cmd_logs "$@"
            ;;
        help|--help|-h)
            cmd_help
            ;;
        *)
            log_error "Unknown command: ${cmd}"
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
