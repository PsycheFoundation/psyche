use std::process::Command;

use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointLoggerPrinter;
use solana_toolbox_endpoint::ToolboxEndpointProgramTestPreloadedProgram;

async fn anchor_build_folder(folder: &str) -> Result<()> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("cd {} && anchor build", folder))
        .output()
        .with_context(|| format!("anchor build in {}", folder))?;
    if !output.status.success() {
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        return Err(anyhow!("Failed to build in {}", folder));
    }
    Ok(())
}

pub async fn create_memnet_endpoint() -> Result<ToolboxEndpoint> {
    anchor_build_folder("../solana-authorizer").await?;
    anchor_build_folder("../solana-coordinator").await?;
    anchor_build_folder("../solana-treasurer").await?;
    let mut endpoint =
        ToolboxEndpoint::new_program_test_with_preloaded_programs(&[
            ToolboxEndpointProgramTestPreloadedProgram {
                id: psyche_solana_authorizer::ID,
                path: "../solana-authorizer/target/deploy/psyche_solana_authorizer",
            },
            ToolboxEndpointProgramTestPreloadedProgram {
                id: psyche_solana_coordinator::ID,
                path: "../solana-coordinator/target/deploy/psyche_solana_coordinator",
            },
            ToolboxEndpointProgramTestPreloadedProgram {
                id: psyche_solana_treasurer::ID,
                path: "../solana-treasurer/target/deploy/psyche_solana_treasurer",
            },
        ])
        .await;
    endpoint.add_logger(Box::new(ToolboxEndpointLoggerPrinter::default()));
    Ok(endpoint)
}
