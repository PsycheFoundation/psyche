FROM psyche-base AS base
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl wget build-essential libfontconfig-dev && rm -rf /var/lib/apt/lists/*

# Copy and set libtorch from base
COPY --from=base /usr/home/libtorch /usr/home/libtorch
ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib

# Copy the psyche client binary from base
COPY --from=base /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin/psyche-solana-client
COPY --from=base /usr/src/psyche/target/release/psyche-centralized-client /usr/local/bin/psyche-centralized-client
COPY --from=base /usr/src/psyche/target/release/examples/inference /usr/local/bin/inference
COPY --from=base /usr/src/psyche/target/release/examples/train /usr/local/bin/train
COPY ./docker/train_entrypoint.sh /usr/local
RUN chmod 755 /usr/local/train_entrypoint.sh

ENV RUST_BACKTRACE=1
ENV RUST_LOG=warn,psyche=info,iroh=error
ENV WRITE_RUST_LOG=info,psyche=debug,iroh=error

ENTRYPOINT ["/usr/local/train_entrypoint.sh"]
