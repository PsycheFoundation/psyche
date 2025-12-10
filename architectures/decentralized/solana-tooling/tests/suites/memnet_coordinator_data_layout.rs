use psyche_solana_coordinator::{
    coordinator_account_from_bytes, CoordinatorAccount,
};

#[tokio::test]
pub async fn run() {
    let coordinator_bytes =
        include_bytes!("../fixtures/coordinator-account-v1.so").to_vec();
    let coordinator_account =
        coordinator_account_from_bytes(&coordinator_bytes).unwrap();
    assert_eq!(
        coordinator_account.version,
        CoordinatorAccount::VERSION,
        "coordinator account version mismatch, expected {} got {}",
        CoordinatorAccount::VERSION,
        coordinator_account.version,
    );
    assert_eq!(
        coordinator_account
            .state
            .coordinator
            .config
            .global_batch_size_start,
        2048,
        "coordinator account data mismatch, storage may have changed"
    );
    assert_eq!(
        coordinator_account.state.metadata.vocab_size, 3033,
        "coordinator account data mismatch, storage may have changed"
    );
    assert_eq!(
        coordinator_account.nonce, 3,
        "coordinator account data mismatch, storage may have changed"
    );
}
