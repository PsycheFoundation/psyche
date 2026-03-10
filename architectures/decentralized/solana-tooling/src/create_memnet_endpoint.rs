use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointLoggerPrinter;
use solana_toolbox_endpoint::toolbox_endpoint_program_test_builtin_program_anchor;

pub async fn create_memnet_endpoint() -> ToolboxEndpoint {
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
            toolbox_endpoint_program_test_builtin_program_anchor!(
                "psyche_solana_mining_pool",
                psyche_solana_mining_pool::ID,
                psyche_solana_mining_pool::entry
            ),
            toolbox_endpoint_program_test_builtin_program_anchor!(
                "psyche_solana_distributor",
                psyche_solana_distributor::ID,
                psyche_solana_distributor::entry
            ),
        ])
        .await;
    endpoint.add_logger(Box::new(ToolboxEndpointLoggerPrinter::default()));
    endpoint
}
