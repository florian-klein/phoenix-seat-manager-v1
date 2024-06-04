mod setup;

use crate::setup::init::bootstrap_default;
use crate::setup::init::PhoenixTestClient;
use ellipsis_client::EllipsisClient;
use phoenix::program::create_deposit_funds_instruction;
use phoenix::program::deposit::DepositParams;
use phoenix_seat_manager::get_authorized_delegate_pda;

use phoenix_seat_manager::instruction_builders::create_add_approved_evictor_instruction;
use phoenix_seat_manager::instruction_builders::create_claim_seat_instruction;
use phoenix_seat_manager::instruction_builders::create_evict_seat_with_authorized_delegate_instruction;
use phoenix_seat_manager::instruction_builders::create_remove_approved_evictor_instruction;
use phoenix_seat_manager::instruction_builders::EvictTraderAccountBackup;
use setup::helpers::airdrop;
use setup::helpers::sol;
use setup::init::bootstrap_default_without_sm_claiming_authority;
use setup::init::setup_account;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

#[tokio::test]
async fn test_add_remove_happy_path() {
    let PhoenixTestClient {
        ctx: _,
        sdk,
        mint_authority: _,
        market: _,
    } = bootstrap_default(5).await;

    let authorized_delegate = Keypair::new();
    let (authorized_delegate_pda, _) =
        get_authorized_delegate_pda(&sdk.client.payer.pubkey(), &authorized_delegate.pubkey());

    // Add authorized delegate evictor
    let add_evictor_ix = create_add_approved_evictor_instruction(
        &sdk.client.payer.pubkey(),
        &authorized_delegate.pubkey(),
    );

    sdk.client
        .sign_send_instructions(vec![add_evictor_ix], vec![])
        .await
        .unwrap();

    let authorized_delegate_pda_data = sdk
        .client
        .get_account(&authorized_delegate_pda)
        .await
        .unwrap();

    assert_ne!(authorized_delegate_pda_data.lamports, 0);
    assert_eq!(
        authorized_delegate_pda_data.owner,
        phoenix_seat_manager::id()
    );

    // Remove authorized delegate evictor
    let remove_evictor_ix = create_remove_approved_evictor_instruction(
        &sdk.client.payer.pubkey(),
        &authorized_delegate.pubkey(),
    );

    sdk.client
        .sign_send_instructions(vec![remove_evictor_ix], vec![])
        .await
        .unwrap();

    let authorized_delegate_pda_resp = sdk.client.get_account(&authorized_delegate_pda).await;

    assert!(authorized_delegate_pda_resp.is_err());
}

#[tokio::test]
async fn test_evict_seat_multiple_authorized() {
    let PhoenixTestClient {
        ctx: _,
        sdk,
        market,
        mint_authority,
    } = bootstrap_default(5).await;

    let authorized_delegate = Keypair::new();
    let (authorized_delegate_pda, _) =
        get_authorized_delegate_pda(&sdk.client.payer.pubkey(), &authorized_delegate.pubkey());

    // Add authorized delegate evictor
    let add_evictor_ix = create_add_approved_evictor_instruction(
        &sdk.client.payer.pubkey(),
        &authorized_delegate.pubkey(),
    );

    sdk.client
        .sign_send_instructions(vec![add_evictor_ix], vec![])
        .await
        .unwrap();

    let authorized_delegate_pda_data = sdk
        .client
        .get_account(&authorized_delegate_pda)
        .await
        .unwrap();

    assert_ne!(authorized_delegate_pda_data.lamports, 0);
    assert_eq!(
        authorized_delegate_pda_data.owner,
        phoenix_seat_manager::id()
    );

    let meta = sdk.get_market_metadata(&market).await.unwrap();

    // Claim seats for two traders, and add deposits
    let trader_one = setup_account(
        &sdk.client,
        &mint_authority,
        meta.base_mint,
        meta.quote_mint,
    )
    .await;

    let trader_two = setup_account(
        &sdk.client,
        &mint_authority,
        meta.base_mint,
        meta.quote_mint,
    )
    .await;

    let claim_seat_one = create_claim_seat_instruction(&trader_one.user.pubkey(), &market);

    let deposit_one = create_deposit_funds_instruction(
        &market,
        &trader_one.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: 1,
            base_lots_to_deposit: 1,
        },
    );

    let claim_seat_two = create_claim_seat_instruction(&trader_two.user.pubkey(), &market);

    let deposit_two = create_deposit_funds_instruction(
        &market,
        &trader_two.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: 1,
            base_lots_to_deposit: 1,
        },
    );

    sdk.client
        .sign_send_instructions(
            vec![claim_seat_one, deposit_one, claim_seat_two, deposit_two],
            vec![&trader_one.user, &trader_two.user],
        )
        .await
        .unwrap();

    let traders = sdk.get_traders_with_market_key(&market).await.unwrap();
    assert!(traders.get(&trader_one.user.pubkey()).is_some());
    assert!(traders.get(&trader_two.user.pubkey()).is_some());

    // Evict seats for both traders
    let evict_seats = create_evict_seat_with_authorized_delegate_instruction(
        &market,
        &meta.base_mint,
        &meta.quote_mint,
        &authorized_delegate.pubkey(),
        vec![
            EvictTraderAccountBackup {
                trader_pubkey: trader_one.user.pubkey(),
                base_token_account_backup: None,
                quote_token_account_backup: None,
            },
            EvictTraderAccountBackup {
                trader_pubkey: trader_two.user.pubkey(),
                base_token_account_backup: None,
                quote_token_account_backup: None,
            },
        ],
        &sdk.client.payer.pubkey(),
    );

    let compute_increase = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

    sdk.client
        .sign_send_instructions(
            vec![compute_increase, evict_seats],
            vec![&authorized_delegate],
        )
        .await
        .unwrap();

    // Assert that neither trader are in the market state
    let traders = sdk.get_traders_with_market_key(&market).await.unwrap();
    assert!(traders.get(&trader_one.user.pubkey()).is_none());
    assert!(traders.get(&trader_two.user.pubkey()).is_none());
}

#[tokio::test]
async fn test_evict_seat_multiple_authorized_fails_after_delegate_removal() {
    let PhoenixTestClient {
        ctx,
        sdk,
        market,
        mint_authority,
    } = bootstrap_default(5).await;

    let authorized_delegate = Keypair::new();
    let (authorized_delegate_pda, _) =
        get_authorized_delegate_pda(&sdk.client.payer.pubkey(), &authorized_delegate.pubkey());

    // Add authorized delegate evictor
    let add_evictor_ix = create_add_approved_evictor_instruction(
        &sdk.client.payer.pubkey(),
        &authorized_delegate.pubkey(),
    );

    sdk.client
        .sign_send_instructions(vec![add_evictor_ix], vec![])
        .await
        .unwrap();

    let authorized_delegate_pda_data = sdk
        .client
        .get_account(&authorized_delegate_pda)
        .await
        .unwrap();

    assert_ne!(authorized_delegate_pda_data.lamports, 0);
    assert_eq!(
        authorized_delegate_pda_data.owner,
        phoenix_seat_manager::id()
    );

    // Remove authorized delegate evictor
    let remove_evictor_ix = create_remove_approved_evictor_instruction(
        &sdk.client.payer.pubkey(),
        &authorized_delegate.pubkey(),
    );

    sdk.client
        .sign_send_instructions(vec![remove_evictor_ix], vec![])
        .await
        .unwrap();

    let authorized_delegate_pda_resp = sdk.client.get_account(&authorized_delegate_pda).await;

    assert!(authorized_delegate_pda_resp.is_err());

    let meta = sdk.get_market_metadata(&market).await.unwrap();

    // Claim seats for two traders, and add deposits
    let trader_one = setup_account(
        &sdk.client,
        &mint_authority,
        meta.base_mint,
        meta.quote_mint,
    )
    .await;

    let trader_two = setup_account(
        &sdk.client,
        &mint_authority,
        meta.base_mint,
        meta.quote_mint,
    )
    .await;

    let claim_seat_one = create_claim_seat_instruction(&trader_one.user.pubkey(), &market);

    let deposit_one = create_deposit_funds_instruction(
        &market,
        &trader_one.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: 1,
            base_lots_to_deposit: 1,
        },
    );

    let claim_seat_two = create_claim_seat_instruction(&trader_two.user.pubkey(), &market);

    let deposit_two = create_deposit_funds_instruction(
        &market,
        &trader_two.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: 1,
            base_lots_to_deposit: 1,
        },
    );

    sdk.client
        .sign_send_instructions(
            vec![claim_seat_one, deposit_one, claim_seat_two, deposit_two],
            vec![&trader_one.user, &trader_two.user],
        )
        .await
        .unwrap();

    let traders = sdk.get_traders_with_market_key(&market).await.unwrap();
    assert!(traders.get(&trader_one.user.pubkey()).is_some());
    assert!(traders.get(&trader_two.user.pubkey()).is_some());

    // Create new client so that we don't accidentally sign with the seat_manager_authority keypair
    airdrop(&sdk.client, &authorized_delegate.pubkey(), sol(10.0))
        .await
        .unwrap();
    let mut ellipsis_client = EllipsisClient::from_banks(&ctx.banks_client, &authorized_delegate)
        .await
        .unwrap();
    let authority = Keypair::new();
    ellipsis_client.add_keypair(&authority);

    // Evict seats for both traders
    let evict_seats = create_evict_seat_with_authorized_delegate_instruction(
        &market,
        &meta.base_mint,
        &meta.quote_mint,
        &authorized_delegate.pubkey(),
        vec![
            EvictTraderAccountBackup {
                trader_pubkey: trader_one.user.pubkey(),
                base_token_account_backup: None,
                quote_token_account_backup: None,
            },
            EvictTraderAccountBackup {
                trader_pubkey: trader_two.user.pubkey(),
                base_token_account_backup: None,
                quote_token_account_backup: None,
            },
        ],
        &sdk.client.payer.pubkey(),
    );

    let compute_increase = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

    ellipsis_client
        .sign_send_instructions(
            vec![compute_increase, evict_seats],
            vec![&authorized_delegate],
        )
        .await
        .unwrap();

    // Assert that both traders are still in the market state, since eviction above should have failed
    let traders = sdk.get_traders_with_market_key(&market).await.unwrap();
    assert!(traders.get(&trader_one.user.pubkey()).is_some());
    assert!(traders.get(&trader_two.user.pubkey()).is_some());
}
