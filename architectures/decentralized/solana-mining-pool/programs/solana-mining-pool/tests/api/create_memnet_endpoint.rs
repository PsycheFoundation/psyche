use solana_toolbox_endpoint::toolbox_endpoint_program_test_builtin_program_anchor;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointLoggerPrinter;

pub async fn create_memnet_endpoint() -> ToolboxEndpoint {
    let mut endpoint =
        ToolboxEndpoint::new_program_test_with_builtin_programs(&[
            toolbox_endpoint_program_test_builtin_program_anchor!(
                "psyche_solana_mining_pool",
                psyche_solana_mining_pool::ID,
                psyche_solana_mining_pool::entry
            ),
        ])
        .await;
    endpoint.add_logger(Box::new(ToolboxEndpointLoggerPrinter::default()));
    endpoint
}
