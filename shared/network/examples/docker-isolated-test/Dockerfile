FROM python:3.12-bookworm AS builder

# Install Rust nightly
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly
ENV PATH="/root/.cargo/bin:${PATH}"

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Install PyTorch (CPU only, for libtorch)
RUN pip install --no-cache-dir torch --index-url https://download.pytorch.org/whl/cpu

# Set libtorch to use PyTorch's installed copy
ENV LIBTORCH_USE_PYTORCH=1
ENV LIBTORCH_BYPASS_VERSION_CHECK=1

WORKDIR /build
COPY . .

RUN cargo build --release --example model_sharing_test -p psyche-network

# Find and copy all libtorch shared libs for the runtime image
RUN mkdir -p /libtorch-libs && \
    TORCH_DIR=$(python3 -c "import torch; print(torch.__path__[0])") && \
    cp ${TORCH_DIR}/lib/lib*.so* /libtorch-libs/

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /libtorch-libs/ /usr/local/lib/
COPY --from=builder /build/target/release/examples/model_sharing_test /usr/local/bin/model_sharing_test
RUN ldconfig

ENTRYPOINT ["model_sharing_test"]
