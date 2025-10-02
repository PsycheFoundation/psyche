# Simulations

## How to run

1. Run `curl -fsSL https://iroh.computer/n0des.sh | sh`, this will download the n0des tool binary that we'll need
2. In one terminal run `n0des dev` using the downloaded binary. This will run the server and a web for the simulatiosn frontend, you can enter going to `http://127.0.0.1:8080`
3. In another terminal run `just run_simulation`, that will start running two simulations created in `simulations.rs`
