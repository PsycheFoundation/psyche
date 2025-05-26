FROM nvidia/cuda:12.4.1-devel-ubuntu22.04 AS base
WORKDIR /usr/src

RUN apt-get update && apt-get install -y \
    unzip libssl-dev libgomp1 curl wget build-essential libfontconfig-dev \
    && rm -rf /var/lib/apt/lists/*

# Download and extract libtorch
RUN wget https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.6.0%2Bcu124.zip \
    && unzip libtorch-cxx11-abi-shared-with-deps-2.6.0+cu124.zip \
    && rm -rf libtorch-cxx11-abi-shared-with-deps-2.6.0+cu124.zip

# Chef planner
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /usr/src/psyche

FROM chef AS planner
COPY . .

# This step will generate a recipe with all the dependencies that need to be installed
RUN cargo chef prepare --bin psyche-solana-client --recipe-path client-recipe.json

# Chef builder
FROM chef AS chef_builder

# Copy libtorch from base
COPY --from=base /usr/src/libtorch /usr/home/libtorch

# Copy cuda shared libraries and headers from base
COPY --from=base /usr/local/cuda /usr/home/cuda
COPY --from=base /usr/include /usr/include

# Set CUDA and libtorch env variables for Psyche client compilation
ENV CUDA_HOME="/usr/home/cuda"
ENV PATH="${CUDA_HOME}/bin:${PATH}"
ENV CPATH="/usr/include:${CUDA_HOME}/include"
ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH="${CUDA_HOME}/lib64:/usr/home/libtorch/lib"

# Copy the recipe from the planner
COPY --from=planner /usr/src/psyche/client-recipe.json client-recipe.json

# Build the project dependencies, this will also create a new layer that caches the dependencies
RUN cargo chef cook --release --recipe-path client-recipe.json

# Build the actual binaries
COPY . .
RUN cargo build -p psyche-solana-client --release --features parallelism
RUN cargo build -p psyche-centralized-client --release --features parallelism
RUN cargo build --example inference --release --features parallelism
RUN cargo build --example train --release --features parallelism