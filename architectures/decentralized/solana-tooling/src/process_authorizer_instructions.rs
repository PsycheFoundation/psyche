use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anyhow::Result;
use psyche_solana_authorizer::accounts::AuthorizationCloseAccounts;
use psyche_solana_authorizer::accounts::AuthorizationCreateAccounts;
use psyche_solana_authorizer::accounts::AuthorizationGranteeUpdateAccounts;
use psyche_solana_authorizer::accounts::AuthorizationGrantorUpdateAccounts;
use psyche_solana_authorizer::find_authorization;
use psyche_solana_authorizer::instruction::AuthorizationClose;
use psyche_solana_authorizer::instruction::AuthorizationCreate;
use psyche_solana_authorizer::instruction::AuthorizationGranteeUpdate;
use psyche_solana_authorizer::instruction::AuthorizationGrantorUpdate;
use psyche_solana_authorizer::logic::AuthorizationCloseParams;
use psyche_solana_authorizer::logic::AuthorizationCreateParams;
use psyche_solana_authorizer::logic::AuthorizationGranteeUpdateParams;
use psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_authorizer_authorization_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    grantor: &Keypair,
    grantee: &Pubkey,
    scope: &[u8],
) -> Result<Pubkey> {
    let authorization = find_authorization(&grantor.pubkey(), grantee, scope);
    let accounts = AuthorizationCreateAccounts {
        payer: payer.pubkey(),
        grantor: grantor.pubkey(),
        authorization,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: AuthorizationCreate {
            params: AuthorizationCreateParams {
                grantee: *grantee,
                scope: scope.to_vec(),
            },
        }
        .data(),
        program_id: psyche_solana_authorizer::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[grantor])
        .await?;
    Ok(authorization)
}

pub async fn process_authorizer_authorization_grantor_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    grantor: &Keypair,
    authorization: &Pubkey,
    params: AuthorizationGrantorUpdateParams,
) -> Result<()> {
    let accounts = AuthorizationGrantorUpdateAccounts {
        grantor: grantor.pubkey(),
        authorization: *authorization,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: AuthorizationGrantorUpdate { params }.data(),
        program_id: psyche_solana_authorizer::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[grantor])
        .await?;
    Ok(())
}

pub async fn process_authorizer_authorization_grantee_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    grantee: &Keypair,
    authorization: &Pubkey,
    params: AuthorizationGranteeUpdateParams,
) -> Result<()> {
    let accounts = AuthorizationGranteeUpdateAccounts {
        payer: payer.pubkey(),
        grantee: grantee.pubkey(),
        authorization: *authorization,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: AuthorizationGranteeUpdate { params }.data(),
        program_id: psyche_solana_authorizer::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[grantee])
        .await?;
    Ok(())
}

pub async fn process_authorizer_authorization_close(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    grantor: &Keypair,
    authorization: &Pubkey,
    spill: &Pubkey,
) -> Result<()> {
    let accounts = AuthorizationCloseAccounts {
        grantor: grantor.pubkey(),
        spill: *spill,
        authorization: *authorization,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: AuthorizationClose {
            params: AuthorizationCloseParams {},
        }
        .data(),
        program_id: psyche_solana_authorizer::ID,
    };
    endpoint
        .process_instruction_with_signers(payer, instruction, &[grantor])
        .await?;
    Ok(())
}
