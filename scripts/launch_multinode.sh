#!/bin/bash

# Slurm Multi-Node Sidecar Launcher
# Usage: sbatch --nodelist=7da1cfaf-50,7da1cfaf-52,7da1cfaf-53 launch_multinode.sh

#SBATCH --job-name=psyche-multinode
#SBATCH --output=multinode_run_%j.out
#SBATCH --error=multinode_run_%j.err
#SBATCH --gres=gpu:8

set -euo pipefail

source .multinode_env
if [[ "${RPC:-}" == "" ]]; then
    echo -e "\n[!] RPC env variable was not set."
    exit 1
fi

if [[ "${WS_RPC:-}" == "" ]]; then
    echo -e "\n[!] WS_RPC env variable was not set."
    exit 1
fi

if [[ "${WALLET_PRIVATE_KEY_PATH:-}" == "" ]]; then
    echo -e "\n[!] WALLET_PRIVATE_KEY_PATH env variable was not set."
    exit 1
fi

if [[ "${DATA_PARALLELISM:-}" == "" ]]; then
    echo -e "\n[!] DATA_PARALLELISM env variable was not set."
    exit 1
fi

if [[ "${HF_MODEL_REPO:-}" == "" ]]; then
    echo -e "\n[!] HF_MODEL_REPO env variable was not set."
    exit 1
fi

PSYCHE_IMPL=${PSYCHE_IMPL:-python}
PSYCHE_WORLD_SIZE=$DATA_PARALLELISM

NODE_LIST=($(scontrol show hostnames "$SLURM_JOB_NODELIST"))
MASTER_NODE="${NODE_LIST[-1]}"

mapfile -t sidecar_nodes < <(scontrol show hostnames "$SLURM_JOB_NODELIST")
unset "sidecar_nodes[-1]"

echo "
Slurm Multi-Node Psyche Sidecar
===============================
Job ID:         $SLURM_JOB_ID
Main Host:      $MASTER_NODE
Current Node:   $SLURMD_NODENAME
World Size:     $PSYCHE_WORLD_SIZE
Implementation: $PSYCHE_IMPL
Node List:      $SLURM_JOB_NODELIST
Model:          $HF_MODEL_REPO
"

echo -e "\n[!] Pulling Psyche client docker images...\n"

for i in ${!NODE_LIST[@]}; do
    node_hostname="${NODE_LIST[$i]}"

    echo -e "\t * Pulling docker image on node $node_hostname\n"

    srun --nodes=1 --nodelist="$node_hostname" \
        --exclusive \
        sudo docker pull nousresearch/psyche-client:latest &

    sleep 1
done

echo -e "\n-------------------------------------\n"
echo -e "Waiting for all nodes to download docker images...\n"
wait

echo -e "[+] Starting Psyche sidecars...\n"
echo -e "---------------------------------------\n"
for i in ${!sidecar_nodes[@]}; do
    sidecar_hostname="${sidecar_nodes[$i]}"
    echo "Starting sidecar in node $sidecar_hostname"
    starting_rank=$((8 + $i * 8))

    srun --nodes=1 --nodelist="$sidecar_hostname" \
        --exclusive \
        --gpus=8 \
        sudo docker run --rm \
        --privileged \
        -v /dev/infiniband:/dev/infiniband \
        -e PSYCHE_MAIN_HOST=$MASTER_NODE \
        -e PSYCHE_WORLD_SIZE=$PSYCHE_WORLD_SIZE \
        -e PSYCHE_START_RANK=$starting_rank \
        -e HF_MODEL_REPO=$HF_MODEL_REPO \
        --shm-size=1g \
        --gpus all \
        --network host \
        nousresearch/psyche-client:latest &

    echo -e "\n------------------------------------------\n"
    sleep 10
done

echo -e "[+] Starting Psyche master node...\n"

raw_wallet_private_key=$(cat $WALLET_PRIVATE_KEY_PATH)

srun --nodes=1 --nodelist="$MASTER_NODE" \
    --exclusive \
    --gpus=8 \
    sudo docker run --rm \
    --privileged \
    -v /dev/infiniband:/dev/infiniband \
    -e RAW_WALLET_PRIVATE_KEY=$raw_wallet_private_key \
    -e DATA_PARALLELISM=$PSYCHE_WORLD_SIZE \
    -e RPC=$RPC \
    -e WS_RPC=$WS_RPC \
    -e RUN_ID="test" \
    -e NVIDIA_DRIVER_CAPABILITIES="all" \
    --shm-size=1g \
    --gpus all \
    --network host \
    nousresearch/psyche-client &

echo "Waiting for all processes..."

wait
echo "All nodes completed work"
