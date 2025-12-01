use solana_toolbox_endpoint::toolbox_endpoint_program_test_builtin_program_anchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn create_memnet_endpoint() -> ToolboxEndpoint {
    ToolboxEndpoint::new_program_test_with_builtin_programs(&[
        toolbox_endpoint_program_test_builtin_program_anchor!(
            "psyche_solana_distributor",
            psyche_solana_distributor::ID,
            psyche_solana_distributor::entry
        ),
    ])
    .await
}
