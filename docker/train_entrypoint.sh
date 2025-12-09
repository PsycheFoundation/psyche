#!/bin/bash

set -o errexit
set -euo pipefail

# check if this should run as a sidecar instead of main training
if [[ "${PSYCHE_MAIN_HOST:-}" != "" ]]; then
    echo "PSYCHE_MAIN_HOST detected - switching to sidecar mode"
    exec /bin/sidecar_entrypoint.sh
fi

# Some sanity checks before starting

if [[ "${NVIDIA_DRIVER_CAPABILITIES:-}" == "" ]]; then
    echo -e "\n[!] The NVIDIA_DRIVER_CAPABILITIES env variable was not set."
    exit 1
fi

if [[ "${RPC:-}" == "" ]]; then
    echo -e "\n[!] The RPC env variable was not set."
    exit 1
fi

if [[ "${WS_RPC:-}" == "" ]]; then
    echo -e "\n[!] The WS_RPC env variable was not set."
    exit 1
fi

if [[ "${RUN_ID:-}" == "" ]]; then
    echo -e "\n[!] The RUN_ID env variable was not set."
    exit 1
fi

PSYCHE_CLIENT_PID=0

# Signal handler function
handle_signal() {
    echo "Received signal, stopping application..."
    if [[ $PSYCHE_CLIENT_PID -ne 0 ]]; then
        kill -TERM "$PSYCHE_CLIENT_PID" 2>/dev/null || true  # Send SIGTERM to the psyche client
        wait "$PSYCHE_CLIENT_PID"  # Wait for the client to terminate
    fi
    exit 0  # Exit script gracefully
}

# Trap SIGTERM and SIGINT to handle container shutdown
trap handle_signal TERM INT

echo "

 !555555555555Y7^.    .~J55Y7^   ^~7J57       :YY: .~?Y55Y?^  ^5J??JY^               .:!?JY?~.  :~
 .^~5BBP!!7?77?5BGJ: ~PP7!JG#B57?G7?GBBJ      ?#Y~JGG?~!JPBBPJ55~?BB#!             :JPGY?JPBBPJ7G7
    7BB5^JYJJJ~ ^G#G?BB^   .?GBBP:  :PBBY    :G5JB#P:    .~5GJ^  :BBB~ ..:..      :G#5:    ~5BBB7
    ?BBBP~:~!?B? ~BBBBG:     ~PB?:.  .YBBY. ^GY7BBG:        .    .GBBYJJ?JY55?~  .PBB^......:7Y?^~^
    ?BBG:.5?7GB? ~BBBBBP??J5PGBBBBG?.  7BB57GJ JBBY              .GBBY:    .7BB5:!BBBGGGGGGGGGGGB#?
    ?BB5 ~B~.?!:!GG77GBBBGP5Y?!!?BB#7   ~GB#J  ?BBP.             .GBB!       ?BB5?BBB?^^^^::::::?#!
    ?BBB7 ~JJJJYJ7:  .:^:.       JBB7    PBB~  :GBBY.            .GBB7       ~BBG^GBBY.         7G:
    ?BBGP5J7777?J7   ^:          Y#P:    PBB~   7BBB5~        .^ .PBB!       !BB? !BBBP7:       .^
 ?Y J#BP.:~!777!^.  ^BBP?~:.   .?B5:  ..~BB#?^~: ~5GBBP?~^^!J5GB^:GB#?.:::.  J#B7?~^5BB#BPYJ??JJ5P.
 !5JY555J7!:        YG5PGBGP5YJ5Y!   ~YYYJ??77?!   :~7JYYJJJ?7YBJ?JJJJ?JJJ~  YPY7~.  :!?YY5YJ7~:?Y.
                   ?G:  .::^^^:.      .        .~77~.  .      !B:
...................?!.......................Y?~::J?::~?5......!Y:...................................
:::::::::::::::::::..:::::::::::::::::::::::5Y!^:??:^!Y5:::::::.::::::::::::::::::::::::::::::::::::
                                            :  .!JJ!.  :
"

RESET_TIME=120  # Reset retries if the client runs for 2 minutes
num_restarts=0

while true; do
    echo -e "\n[+] Starting to train in run ${RUN_ID}..."

    start_time=$SECONDS  # Record start time

    psyche-solana-client train \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --logs "console" &

    PSYCHE_CLIENT_PID=$!
    set +e
    wait "$PSYCHE_CLIENT_PID"
    EXIT_STATUS=$?
    set -e

    duration=$((SECONDS - start_time))  # Calculate runtime duration
    echo -e "\n[!] Psyche client exited with status '$EXIT_STATUS'."

    # Reset PID after client exits
    PSYCHE_CLIENT_PID=0

    # Exit code 10 means version mismatch - exit container immediately (no retry)
    if [[ $EXIT_STATUS -eq 10 ]]; then
        echo -e "[!] Version mismatch detected. Exiting container without retry..."
        exit 10
    fi

    # Reset restart counter if client ran longer than RESET_TIME
    if [ $duration -ge $RESET_TIME ]; then
        num_restarts=0
        echo "Client ran successfully for ${RESET_TIME}+ seconds - resetting restart counter"
    else
        ((num_restarts += 1))
    fi

    if [[ $num_restarts -ge 5 ]]; then
        echo -e "[!] Maximum restarts reached. Exiting..."
        exit 1;
    fi

    echo "Waiting 5 seconds before restart..."
    sleep 5
done
