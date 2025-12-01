#!/bin/bash

set -e

echo "Deteniendo todos los contenedores de Docker..."
docker stop $(docker ps -q) 2>/dev/null || echo "No hay contenedores corriendo"

echo "Limpiando sistema Docker..."
docker system prune -a -f

echo "Deteniendo procesos de Solana validator..."
pkill -f solana-test-validator || echo "No hay procesos de validator corriendo"

echo "Configurando entorno de desarrollo en tmux..."

# Nombre de la sesión de tmux
SESSION_NAME="psyche-dev"

# Crear nueva sesión de tmux con la primera ventana
tmux new-session -d -s "$SESSION_NAME" -n "telemetry"

# Ventana 0: Docker Compose Telemetry
tmux send-keys -t "$SESSION_NAME:0" "cd /tmp/peter/psyche && docker compose -f telemetry/docker-compose.yml up" C-m

# Ventana 1: Setup Solana Localnet
tmux new-window -t "$SESSION_NAME:1" -n "solana-setup"
tmux send-keys -t "$SESSION_NAME:1" "cd /tmp/peter/psyche && nix develop -c bash -c 'just setup-solana-localnet-light-test-run'" C-m

echo "Esperando 30 segundos para que el setup de Solana termine..."
sleep 30

# Ventana 2: Cliente 1
tmux new-window -t "$SESSION_NAME:2" -n "client-1"
tmux send-keys -t "$SESSION_NAME:2" "cd /tmp/peter/psyche" C-m
tmux send-keys -t "$SESSION_NAME:2" "export OTLP_METRICS_URL=\"http://localhost:4318/v1/metrics\"" C-m
tmux send-keys -t "$SESSION_NAME:2" "export OTLP_LOGS_URL=\"http://localhost:4318/v1/logs\"" C-m
tmux send-keys -t "$SESSION_NAME:2" "nix develop -c bash -c 'just start-training-localnet-light-client-telemetry'" C-m

echo "Esperando 10 segundos antes de iniciar el segundo cliente..."
sleep 10

# Ventana 3: Cliente 2
tmux new-window -t "$SESSION_NAME:3" -n "client-2"
tmux send-keys -t "$SESSION_NAME:3" "cd /tmp/peter/psyche" C-m
tmux send-keys -t "$SESSION_NAME:3" "export OTLP_METRICS_URL=\"http://localhost:4318/v1/metrics\"" C-m
tmux send-keys -t "$SESSION_NAME:3" "export OTLP_LOGS_URL=\"http://localhost:4318/v1/logs\"" C-m
tmux send-keys -t "$SESSION_NAME:3" "nix develop -c bash -c 'just start-training-localnet-light-client-telemetry'" C-m

# Volver a la primera ventana
tmux select-window -t "$SESSION_NAME:0"

echo "Entorno configurado!"
echo "Ejecuta: tmux attach -t $SESSION_NAME"
echo ""
echo "Ventanas disponibles:"
echo "  0: telemetry (docker-compose)"
echo "  1: solana-setup"
echo "  2: client-1"
echo "  3: client-2"
echo ""
echo "Navega entre ventanas con: Ctrl+b [0-3]"
echo "Para salir sin cerrar: Ctrl+b d"
