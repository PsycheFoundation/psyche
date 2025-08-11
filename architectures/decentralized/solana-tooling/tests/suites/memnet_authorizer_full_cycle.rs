use psyche_solana_authorizer::find_authorization;
use psyche_solana_authorizer::logic::AuthorizationGranteeUpdateParams;
use psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::get_accounts::get_authorization;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_close;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_create;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantee_update;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantor_update;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint
        .request_airdrop(&payer.pubkey(), 5_000_000_000)
        .await
        .unwrap();

    // The accounts involved in our authorization
    let grantor = Keypair::new();
    let grantee = Keypair::new();
    let scope = vec![1, 2, 3, 4, 5, 6, 7];

    // Dummy delegates users
    let mut delegates = vec![];
    for _ in 0..66 {
        delegates.push(Pubkey::new_unique());
    }

    // Authorization PDA doesnt exist at the start
    assert!(get_authorization(
        &mut endpoint,
        &find_authorization(&grantor.pubkey(), &grantee.pubkey(), &scope)
    )
    .await
    .unwrap()
    .is_none());

    // Create the authorization
    let authorization = process_authorizer_authorization_create(
        &mut endpoint,
        &payer,
        &grantor,
        &grantee.pubkey(),
        &scope,
    )
    .await
    .unwrap();

    // Authorization PDA has proper keys, scope, validity and delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert!(!authorization_state.active);
    assert_eq!(authorization_state.delegates, vec![]);

    // Check the function is_valid_for returns the expected values
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &grantee.pubkey(),
        &scope
    ));
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[1],
        &scope
    ));

    // The grantee can now set the delegates
    process_authorizer_authorization_grantee_update(
        &mut endpoint,
        &payer,
        &grantee,
        &authorization,
        AuthorizationGranteeUpdateParams {
            delegates_clear: false,
            delegates_added: delegates[..5].to_vec(),
        },
    )
    .await
    .unwrap();

    // Authorization PDA has proper keys, scope, validity and delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert!(!authorization_state.active);
    assert_eq!(authorization_state.delegates, delegates[..5]);

    // Check the function is_valid_for returns the expected values
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &grantee.pubkey(),
        &scope
    ));
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[1],
        &scope
    ));

    // The grantee can increase the set the delegates
    process_authorizer_authorization_grantee_update(
        &mut endpoint,
        &payer,
        &grantee,
        &authorization,
        AuthorizationGranteeUpdateParams {
            delegates_clear: true,
            delegates_added: delegates[10..30].to_vec(),
        },
    )
    .await
    .unwrap();
    process_authorizer_authorization_grantee_update(
        &mut endpoint,
        &payer,
        &grantee,
        &authorization,
        AuthorizationGranteeUpdateParams {
            delegates_clear: false,
            delegates_added: delegates[30..50].to_vec(),
        },
    )
    .await
    .unwrap();

    // Authorization PDA has proper keys, scope, validity and delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert!(!authorization_state.active);
    assert_eq!(authorization_state.delegates, delegates[10..50]);

    // Check the function is_valid_for returns the expected values
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[40],
        &scope
    ));

    // The grantor can enable the authorization at any time
    process_authorizer_authorization_grantor_update(
        &mut endpoint,
        &payer,
        &grantor,
        &authorization,
        AuthorizationGrantorUpdateParams { active: true },
    )
    .await
    .unwrap();

    // Authorization PDA has proper keys, scope, validity and delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert!(authorization_state.active);
    assert_eq!(authorization_state.delegates, delegates[10..50]);

    // Check the function is_valid_for returns the expected values
    assert!(authorization_state.is_valid_for(
        &grantor.pubkey(),
        &grantee.pubkey(),
        &scope
    ));
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[3],
        &scope
    ));
    assert!(authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[45],
        &scope
    ));

    // The grantee can decrease the set the delegates
    process_authorizer_authorization_grantee_update(
        &mut endpoint,
        &payer,
        &grantee,
        &authorization,
        AuthorizationGranteeUpdateParams {
            delegates_clear: true,
            delegates_added: delegates[3..5].to_vec(),
        },
    )
    .await
    .unwrap();

    // Authorization PDA has proper keys, scope, validity and delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert!(authorization_state.active);
    assert_eq!(authorization_state.delegates, delegates[3..5]);

    // Check the function is_valid_for returns the expected values
    assert!(authorization_state.is_valid_for(
        &grantor.pubkey(),
        &grantee.pubkey(),
        &scope
    ));
    assert!(authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[3],
        &scope
    ));
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[45],
        &scope
    ));

    // The grantor can disable the authorization at any time
    process_authorizer_authorization_grantor_update(
        &mut endpoint,
        &payer,
        &grantor,
        &authorization,
        AuthorizationGrantorUpdateParams { active: false },
    )
    .await
    .unwrap();

    // Authorization PDA has proper keys, scope, validity and delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert!(!authorization_state.active);
    assert_eq!(authorization_state.delegates, delegates[3..5]);

    // Check the function is_valid_for returns the expected values
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &grantee.pubkey(),
        &scope
    ));
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[3],
        &scope
    ));
    assert!(!authorization_state.is_valid_for(
        &grantor.pubkey(),
        &delegates[45],
        &scope
    ));

    // The grantor can only close the authorization once all the delegate has been cleared
    process_authorizer_authorization_close(
        &mut endpoint,
        &payer,
        &grantor,
        &authorization,
        &payer.pubkey(),
    )
    .await
    .unwrap_err();

    // The grantee can clear the delegate to claw back the rent
    process_authorizer_authorization_grantee_update(
        &mut endpoint,
        &payer,
        &grantee,
        &authorization,
        AuthorizationGranteeUpdateParams {
            delegates_clear: true,
            delegates_added: vec![],
        },
    )
    .await
    .unwrap();

    // The grantor can now close the authorization
    process_authorizer_authorization_close(
        &mut endpoint,
        &payer,
        &grantor,
        &authorization,
        &payer.pubkey(),
    )
    .await
    .unwrap();

    // Authorization PDA must not exist anymore
    assert!(get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .is_none());
}
