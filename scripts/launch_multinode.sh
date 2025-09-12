#!/bin/bash

# Slurm Multi-Node Sidecar Launcher
# Usage: sbatch --nodes=4 slurm_sidecar_launcher.sh

#SBATCH --job-name=psyche-multinode
#SBATCH --ntasks-per-node=1

set -euo pipefail

PSYCHE_IMPL=${PSYCHE_IMPL:-python}

# get the main node hostname (first node in allocation)
MASTER_NODE=$(scontrol show hostnames "$SLURM_JOB_NODELIST" | head -n 1)
export PSYCHE_MAIN_HOST=$MASTER_NODE

# calculate total world size
export PSYCHE_WORLD_SIZE=$SLURM_JOB_NUM_NODES

# calculate rank based on node ID
NODE_LIST=($(scontrol show hostnames "$SLURM_JOB_NODELIST"))
for i in "${!NODE_LIST[@]}"; do
    if [[ "${NODE_LIST[$i]}" == "$SLURMD_NODENAME" ]]; then
        export PSYCHE_RANK=$i
        break
    fi
done

echo "
Slurm Multi-Node Psyche Sidecar
===============================
Job ID:         $SLURM_JOB_ID
Main Host:      $PSYCHE_MAIN_HOST
Current Node:   $SLURMD_NODENAME
Rank:           $PSYCHE_RANK
World Size:     $PSYCHE_WORLD_SIZE
Implementation: $PSYCHE_IMPL
Node List:      $SLURM_JOB_NODELIST
"

# set resource limits based on Slurm allocation
DOCKER_CPUS=${SLURM_CPUS_PER_TASK}

# GPU assignment from Slurm
GPU_DEVICES="${SLURM_STEP_GPUS:-$SLURM_JOB_GPUS}"
if [[ -n "$GPU_DEVICES" ]]; then
    GPU_ARG="--gpus=\"device=${GPU_DEVICES}\""
else
    echo "Warning: No GPUs assigned by Slurm, using --gpus all"
    GPU_ARG="--gpus all"
fi

# node 0 runs the main training process
if [[ $PSYCHE_RANK -eq 0 ]]; then
    echo "Starting main training process on master node..."
    # Use the main training container with custom init method
    exec docker run --rm \
        $GPU_ARG \
        ${SLURM_CPUS_PER_TASK:+--cpus="$SLURM_CPUS_PER_TASK"} \
        --network host \
        -e NVIDIA_DRIVER_CAPABILITIES=all \
        -e RPC="${RPC:-http://localhost:8899}" \
        -e WS_RPC="${WS_RPC:-ws://localhost:8900}" \
        -e RUN_ID="${RUN_ID:-test-multinode}" \
        -e PSYCHE_INIT_METHOD="tcp://0.0.0.0:34567" \
        psyche-solana-client
else
    echo "Starting sidecar process on worker node..."
    # Use the sidecar container
    exec docker run --rm \
        $GPU_ARG \
        ${SLURM_CPUS_PER_TASK:+--cpus="$SLURM_CPUS_PER_TASK"} \
        --network host \
        -e NVIDIA_DRIVER_CAPABILITIES=all \
        -e PSYCHE_MAIN_HOST="$PSYCHE_MAIN_HOST" \
        -e PSYCHE_WORLD_SIZE="$PSYCHE_WORLD_SIZE" \
        -e PSYCHE_RANK="$PSYCHE_RANK" \
        -e PSYCHE_IMPL="$PSYCHE_IMPL" \
        psyche-solana-client
fi
