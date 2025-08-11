use anyhow::Result;
use solana_toolbox_endpoint::toolbox_endpoint_program_test_builtin_program_anchor;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointLoggerPrinter;

pub async fn create_memnet_endpoint() -> Result<ToolboxEndpoint> {
    let mut endpoint =
        ToolboxEndpoint::new_program_test_with_builtin_programs(&[
            toolbox_endpoint_program_test_builtin_program_anchor!(
                "psyche_solana_authorizer",
                psyche_solana_authorizer::ID,
                psyche_solana_authorizer::entry
            ),
            toolbox_endpoint_program_test_builtin_program_anchor!(
                "psyche_solana_coordinator",
                psyche_solana_coordinator::ID,
                psyche_solana_coordinator::entry
            ),
            toolbox_endpoint_program_test_builtin_program_anchor!(
                "psyche_solana_treasurer",
                psyche_solana_treasurer::ID,
                psyche_solana_treasurer::entry
            ),
        ])
        .await;
    endpoint.add_logger(Box::new(ToolboxEndpointLoggerPrinter::default()));
    Ok(endpoint)
}
