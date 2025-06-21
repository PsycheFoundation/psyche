use psyche_solana_coordinator::coordinator_account_from_bytes;

#[tokio::test]
pub async fn run() {
    let coordinator_bytes =
        include_bytes!("../fixtures/coordinator-account-v0.so");
    let coordinator_account =
        coordinator_account_from_bytes(coordinator_bytes).unwrap();
    assert_eq!(coordinator_account.nonce, 563234);
}
