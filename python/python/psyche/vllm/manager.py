"""
vLLM with Updater Manager

Manages the full lifecycle:
1. Start vLLM engine (direct or server mode)
2. Share model memory
3. Spawn updater daemon
4. Provide update interface
5. Clean shutdown
"""

import logging
import subprocess
import time
import torch
import torch.multiprocessing as mp
from typing import Dict, Any, Optional
from pathlib import Path

from .engine import UpdatableLLMEngine
from .updater import spawn_updater_process
from .transforms import build_full_transform_config_llama

logger = logging.getLogger(__name__)


class VLLMWithUpdater:
    """
    High-level manager for vLLM with live weight updates.

    This is the main class that Rust should interact with for Atropos integration.

    Supports two modes:
    - Direct: Uses LLMEngine API directly (for internal use by updater)
    - Server: Spawns OpenAI-compatible API server (for Atropos environments)
    """

    def __init__(
        self,
        model_name: str,
        tensor_parallel_size: int = 1,
        gpu_memory_utilization: float = 0.5,
        max_model_len: Optional[int] = None,
        transform_config: Optional[Dict[str, Dict[str, Any]]] = None,
        update_mode: str = "delta",
        mode: str = "direct",
        server_port: Optional[int] = None,
    ):
        """
        Args:
            model_name: HuggingFace model name or path
            tensor_parallel_size: Number of GPUs for tensor parallelism
            gpu_memory_utilization: Fraction of GPU memory to use (0-1)
            max_model_len: Maximum sequence length
            transform_config: Weight transformation configs. If None, will be
                inferred from model config (assumes LLaMA-style)
            update_mode: "delta" (w += Δw) or "full" (w = w_new)
            mode: "direct" or "server"
            server_port: Port for OpenAI API server (required if mode="server")
        """
        logger.info(f"Initializing VLLMWithUpdater for {model_name}")
        logger.info(f"Mode: {mode}, Update mode: {update_mode}")

        self.model_name = model_name
        self.mode = mode
        self.server_port = server_port
        self.server_process: Optional[subprocess.Popen] = None
        self.engine: Optional[UpdatableLLMEngine] = None
        self.updater_process: Optional[mp.Process] = None
        self.weight_queue: Optional[mp.Queue] = None

        if mode == "server":
            if server_port is None:
                raise ValueError("server_port required for mode='server'")
            self._start_server_mode(
                model_name,
                tensor_parallel_size,
                gpu_memory_utilization,
                max_model_len,
                server_port,
            )
        elif mode == "direct":
            self._start_direct_mode(
                model_name,
                tensor_parallel_size,
                gpu_memory_utilization,
                max_model_len,
                transform_config,
                update_mode,
            )
        else:
            raise ValueError(f"Unknown mode: {mode}")

        logger.info("VLLMWithUpdater initialized successfully")

    def _start_direct_mode(
        self,
        model_name: str,
        tensor_parallel_size: int,
        gpu_memory_utilization: float,
        max_model_len: Optional[int],
        transform_config: Optional[Dict[str, Dict[str, Any]]],
        update_mode: str,
    ):
        """Initialize in direct mode (engine API)"""
        logger.info("Starting in direct mode")

        # Initialize vLLM engine
        self.engine = UpdatableLLMEngine(
            model_name=model_name,
            tensor_parallel_size=tensor_parallel_size,
            gpu_memory_utilization=gpu_memory_utilization,
            max_model_len=max_model_len,
        )

        # Get model and move to shared memory
        model = self.engine.get_model()
        self.engine.share_memory()

        # Infer transform config if not provided
        if transform_config is None:
            logger.info("Inferring transform config for LLaMA-style model")
            # Try to get model config
            try:
                if hasattr(model, "config"):
                    model_config = {
                        "n_layers": getattr(model.config, "num_hidden_layers", 32),
                        "n_heads": getattr(model.config, "num_attention_heads", 32),
                        "n_kv_heads": getattr(
                            model.config,
                            "num_key_value_heads",
                            getattr(model.config, "num_attention_heads", 32),
                        ),
                        "hidden_dim": getattr(model.config, "hidden_size", 4096),
                    }
                    transform_config = build_full_transform_config_llama(model_config)
                else:
                    logger.warning(
                        "Could not infer model config, using empty transform config"
                    )
                    transform_config = {}
            except Exception as e:
                logger.warning(f"Error inferring transform config: {e}")
                transform_config = {}

        # Create update queue
        self.weight_queue = mp.Queue()

        # Spawn updater process
        self.updater_process = spawn_updater_process(
            model=model,
            weight_queue=self.weight_queue,
            transform_config=transform_config,
            update_mode=update_mode,
        )

    def _start_server_mode(
        self,
        model_name: str,
        tensor_parallel_size: int,
        gpu_memory_utilization: float,
        max_model_len: Optional[int],
        server_port: int,
    ):
        """
        Initialize in server mode (OpenAI API).

        This spawns vLLM's built-in OpenAI API server as a subprocess.
        The updater will connect to the server's internal model.

        NOTE: Server mode with updater is more complex and will be implemented
        in a future phase. For now, this just starts the server without updates.
        """
        logger.info(f"Starting vLLM server on port {server_port}")

        # Build vLLM server command
        cmd = [
            "python",
            "-m",
            "vllm.entrypoints.openai.api_server",
            "--model",
            model_name,
            "--port",
            str(server_port),
            "--tensor-parallel-size",
            str(tensor_parallel_size),
            "--gpu-memory-utilization",
            str(gpu_memory_utilization),
        ]

        if max_model_len is not None:
            cmd.extend(["--max-model-len", str(max_model_len)])

        # Start server process
        logger.info(f"Running: {' '.join(cmd)}")
        self.server_process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        # Wait for server to be ready
        logger.info("Waiting for server to be ready...")
        time.sleep(10)  # TODO: Better readiness check

        # Check if process is still running
        if self.server_process.poll() is not None:
            # Process terminated
            stdout, stderr = self.server_process.communicate()
            logger.error(f"Server failed to start!")
            logger.error(f"stdout: {stdout.decode()}")
            logger.error(f"stderr: {stderr.decode()}")
            raise RuntimeError("vLLM server failed to start")

        logger.info(f"vLLM server started on http://localhost:{server_port}")

    def update_weights(self, weight_delta: Dict[str, torch.Tensor]):
        """
        Send weight update to updater daemon.

        Args:
            weight_delta: {param_name: delta_tensor}

        Raises:
            RuntimeError: If called in server mode (not yet supported)
        """
        if self.mode == "server":
            raise RuntimeError(
                "Weight updates in server mode not yet implemented. "
                "Use mode='direct' for now."
            )

        if self.weight_queue is None:
            raise RuntimeError("Weight queue not initialized")

        self.weight_queue.put(weight_delta)
        logger.debug(f"Queued update for {len(weight_delta)} parameters")

    def checkpoint(self):
        """
        Signal updater to checkpoint current state.

        This creates a snapshot of current weights for error recovery.
        """
        if self.mode == "server":
            logger.warning("Checkpoint not supported in server mode")
            return

        if self.weight_queue is None:
            raise RuntimeError("Weight queue not initialized")

        self.weight_queue.put("CHECKPOINT")
        logger.info("Sent checkpoint signal to updater")

    def shutdown(self):
        """Clean shutdown of all components"""
        logger.info("Shutting down VLLMWithUpdater")

        if self.mode == "direct":
            # Stop updater process
            if self.weight_queue is not None:
                self.weight_queue.put("SHUTDOWN")

            if self.updater_process is not None:
                self.updater_process.join(timeout=5)

                if self.updater_process.is_alive():
                    logger.warning(
                        "Updater process did not terminate gracefully, killing"
                    )
                    self.updater_process.terminate()
                    self.updater_process.join()

        elif self.mode == "server":
            # Stop server process
            if self.server_process is not None:
                logger.info("Terminating vLLM server")
                self.server_process.terminate()

                try:
                    self.server_process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    logger.warning("Server did not terminate gracefully, killing")
                    self.server_process.kill()
                    self.server_process.wait()

        logger.info("VLLMWithUpdater shutdown complete")

    def __del__(self):
        """Ensure cleanup on garbage collection"""
        if (
            hasattr(self, "updater_process")
            and self.updater_process is not None
            and self.updater_process.is_alive()
        ):
            self.shutdown()

        if (
            hasattr(self, "server_process")
            and self.server_process is not None
            and self.server_process.poll() is None
        ):
            self.shutdown()

    def __enter__(self):
        """Context manager entry"""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit"""
        self.shutdown()
        return False


def create_vllm_for_training(
    model_name: str,
    tensor_parallel_size: int = 1,
    gpu_memory_utilization: float = 0.5,
    max_model_len: Optional[int] = None,
) -> VLLMWithUpdater:
    """
    Convenience function to create vLLM instance for training use.

    This creates a vLLM instance in direct mode with weight update support,
    suitable for use alongside Psyche training clients.

    Args:
        model_name: HuggingFace model name
        tensor_parallel_size: Number of GPUs for tensor parallelism
        gpu_memory_utilization: Fraction of GPU memory to use
        max_model_len: Maximum sequence length

    Returns:
        VLLMWithUpdater instance ready for training

    Example:
        >>> with create_vllm_for_training("meta-llama/Llama-2-7b-hf") as vllm:
        ...     # Training loop
        ...     for step in range(num_steps):
        ...         # ... training code ...
        ...         weight_delta = compute_weight_delta(model, reference_model)
        ...         vllm.update_weights(weight_delta)
    """
    return VLLMWithUpdater(
        model_name=model_name,
        tensor_parallel_size=tensor_parallel_size,
        gpu_memory_utilization=gpu_memory_utilization,
        max_model_len=max_model_len,
        mode="direct",
    )


def create_vllm_for_atropos(
    model_name: str,
    server_port: int = 9001,
    tensor_parallel_size: int = 1,
    gpu_memory_utilization: float = 0.5,
    max_model_len: Optional[int] = None,
) -> VLLMWithUpdater:
    """
    Convenience function to create vLLM server for Atropos integration.

    This creates a vLLM OpenAI API server suitable for use with Atropos
    environments. Weight updates are not yet supported in server mode.

    Args:
        model_name: HuggingFace model name
        server_port: Port for OpenAI API server
        tensor_parallel_size: Number of GPUs for tensor parallelism
        gpu_memory_utilization: Fraction of GPU memory to use
        max_model_len: Maximum sequence length

    Returns:
        VLLMWithUpdater instance in server mode

    Example:
        >>> with create_vllm_for_atropos("meta-llama/Llama-2-7b-hf", 9001) as vllm:
        ...     # Start Atropos environment pointing to http://localhost:9001
        ...     # python environments/gsm8k_server.py serve \\
        ...     #   --openai.base_url http://localhost:9001/v1
    """
    return VLLMWithUpdater(
        model_name=model_name,
        server_port=server_port,
        tensor_parallel_size=tensor_parallel_size,
        gpu_memory_utilization=gpu_memory_utilization,
        max_model_len=max_model_len,
        mode="server",
    )
