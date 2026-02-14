use anchor_client::solana_sdk::{
    native_token::{lamports_to_sol, sol_to_lamports},
    signature::{EncodableKey, Keypair, Signer},
};
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use solana_client::rpc_client::RpcClient;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum WalletCommands {
    /// Display the pubkey from a keypair file
    Pubkey {
        /// Filepath to a keypair
        keypair_path: PathBuf,
    },
    /// Generate new keypair from a random seed phrase and write to a file
    New {
        /// Filepath to write the new keypair to
        output_path: PathBuf,
    },
    /// Request SOL from a faucet (won't work on mainnet)
    Airdrop {
        /// Amount of SOL to request
        amount: f64,
        /// Path to .env file containing RPC and WALLET_FILE
        #[arg(long, required = true)]
        env_file: PathBuf,
    },
    /// Get the balance of the wallet
    Balance {
        /// Path to .env file containing RPC and WALLET_FILE
        #[arg(long, required = true)]
        env_file: PathBuf,
    },
}

pub fn execute(command: WalletCommands) -> Result<()> {
    match command {
        WalletCommands::Pubkey { keypair_path } => {
            let keypair = Keypair::read_from_file(&keypair_path).map_err(|e| {
                anyhow::anyhow!("Failed to read keypair from {keypair_path:?}: {e}")
            })?;
            println!("{}", keypair.pubkey());
            Ok(())
        }
        WalletCommands::New { output_path } => {
            // this uses OsRng, which is a cryptographically secure RNG,
            // so it's safe.
            let keypair = Keypair::new();

            if output_path.exists() {
                bail!("keypair output path {output_path:?} exists, refusing to overwrite it.")
            }

            keypair
                .write_to_file(&output_path)
                .map_err(|e| anyhow::anyhow!("Failed to write keypair to {output_path:?}: {e}"))?;

            println!("Wrote keypair to {output_path:?}");
            println!("pubkey: {}", keypair.pubkey());
            Ok(())
        }
        WalletCommands::Airdrop { amount, env_file } => airdrop(amount, env_file),
        WalletCommands::Balance { env_file } => check_balance(env_file),
    }
}

/// load an env file and extract the RPC URL and keypair
fn load_env_and_keypair(env_file: &PathBuf) -> Result<(String, Keypair)> {
    crate::load_and_apply_env_file(env_file)?;

    let rpc_url = crate::get_env_var("RPC")?;

    let wallet_file = crate::get_env_var("WALLET_PRIVATE_KEY_PATH")?;

    // expand tilde in wallet path. we could use a crate like `dirs` to get the home dir more reliably, etc,
    // but this will do for now.
    let wallet_path = if wallet_file.starts_with("~") {
        let home = std::env::var("HOME").map_err(|_| {
            anyhow::anyhow!("wallet path contains ~, but HOME environment variable isn't set")
        })?;
        PathBuf::from(wallet_file.replacen("~", &home, 1))
    } else {
        PathBuf::from(wallet_file)
    };

    let keypair = Keypair::read_from_file(&wallet_path)
        .map_err(|e| anyhow::anyhow!("Failed to read keypair from {wallet_path:?}: {e}"))?;

    Ok((rpc_url, keypair))
}

fn airdrop(amount: f64, env_file: PathBuf) -> Result<()> {
    let (rpc_url, keypair) = load_env_and_keypair(&env_file)?;

    let pubkey = keypair.pubkey();

    let rpc_client = RpcClient::new(&rpc_url);

    let lamports = sol_to_lamports(amount);

    println!("Requesting airdrop of {amount} SOL to {pubkey} via RPC {rpc_url}");

    let pre_balance = rpc_client
        .get_balance(&pubkey)
        .context("Failed to get balance before airdrop")?;

    let signature = rpc_client
        .request_airdrop(&pubkey, lamports)
        .context("Failed to request airdrop")?;

    println!("Airdrop requested. Waiting for confirmation..: {signature}");

    let mut confirmed = false;
    for _ in 0..30 {
        print!(".");
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Ok(status) = rpc_client.get_signature_statuses(&[signature]) {
            if let Some(status) = &status.value[0] {
                if status.err.is_none() {
                    confirmed = true;
                    break;
                }
            }
        }
    }
    println!();

    if !confirmed {
        println!("Warning: Airdrop confirmation timed out");
        println!("Run `solana confirm {signature}` to check status");
        return Ok(());
    }

    let post_balance = rpc_client
        .get_balance(&pubkey)
        .context("Failed to get balance post-airdrop")?;

    if post_balance < pre_balance + lamports {
        bail!("Balance unchanged. Run `solana confirm -v {signature}` to debug");
    } else {
        let balance_sol = lamports_to_sol(post_balance);
        println!("New balance: {balance_sol} SOL");
    }

    Ok(())
}

fn check_balance(env_file: PathBuf) -> Result<()> {
    let (rpc_url, keypair) = load_env_and_keypair(&env_file)?;

    let pubkey = keypair.pubkey();

    let rpc_client = RpcClient::new(rpc_url);

    let balance = rpc_client
        .get_balance(&pubkey)
        .context("Failed to get balance")?;

    let balance_sol = lamports_to_sol(balance);

    println!("pubkey {pubkey} has sol balance");
    println!("{balance_sol} SOL");

    Ok(())
}
