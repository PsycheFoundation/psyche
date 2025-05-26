FROM psyche-base AS base
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl wget build-essential iproute2 libfontconfig-dev && rm -rf /var/lib/apt/lists/*

# Install solana
RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

# Copy and set libtorch from base
COPY --from=base /usr/home/libtorch /usr/home/libtorch

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib

ENV RUST_BACKTRACE=1

COPY --from=base /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin

# Copy the entrypoint scripts from host machine.
COPY ./docker/test/client_test_entrypoint.sh /usr/local
RUN chmod 755 /usr/local/client_test_entrypoint.sh

COPY ./docker/test/run_owner_entrypoint.sh /usr/local
RUN chmod 755 /usr/local/run_owner_entrypoint.sh

ENTRYPOINT ["/usr/local/client_test_entrypoint.sh"]
