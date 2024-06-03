use ellipsis_client::{EllipsisClient, EllipsisClientResult};
use phoenix_sdk::sdk_client::SDKClient;
use phoenix_seat_manager::instruction_builders::create_claim_seat_instruction;
use solana_program::native_token::LAMPORTS_PER_SOL;
use solana_sdk::{
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Signature,
    signer::{keypair::Keypair, Signer},
    system_instruction,
};
use spl_token::state::Mint;

use crate::setup::init::setup_account;

use super::init::PhoenixTestAccount;

pub fn sol(amount: f64) -> u64 {
    (amount * LAMPORTS_PER_SOL as f64) as u64
}

pub async fn airdrop(
    context: &EllipsisClient,
    receiver: &Pubkey,
    amount: u64,
) -> EllipsisClientResult<Signature> {
    let ixs = vec![system_instruction::transfer(
        &context.payer.pubkey(),
        receiver,
        amount,
    )];

    context.sign_send_instructions(ixs, vec![]).await
}

pub fn clone_keypair(keypair: &Keypair) -> Keypair {
    Keypair::from_bytes(&keypair.to_bytes()).unwrap()
}

pub async fn create_associated_token_account(
    context: &EllipsisClient,
    wallet: &Pubkey,
    token_mint: &Pubkey,
    token_program: &Pubkey,
) -> EllipsisClientResult<Pubkey> {
    let ixs = vec![
        spl_associated_token_account::instruction::create_associated_token_account(
            &context.payer.pubkey(),
            wallet,
            token_mint,
            token_program,
        ),
    ];
    context.sign_send_instructions(ixs, vec![]).await?;

    Ok(spl_associated_token_account::get_associated_token_address(
        wallet, token_mint,
    ))
}

pub async fn create_mint(
    context: &EllipsisClient,
    authority: &Pubkey,
    freeze_authority: Option<&Pubkey>,
    decimals: u8,
    mint: Option<Keypair>,
) -> EllipsisClientResult<Keypair> {
    let mint = mint.unwrap_or_else(Keypair::new);

    let ixs = vec![
        system_instruction::create_account(
            &context.payer.pubkey(),
            &mint.pubkey(),
            context.rent_exempt(Mint::LEN),
            Mint::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_mint(
            &spl_token::id(),
            &mint.pubkey(),
            authority,
            freeze_authority,
            decimals,
        )
        .unwrap(),
    ];

    context
        .sign_send_instructions(ixs, vec![&context.payer, &mint])
        .await
        .unwrap();
    Ok(mint)
}

pub async fn mint_tokens(
    context: &EllipsisClient,
    authority: &Keypair,
    mint: &Pubkey,
    account: &Pubkey,
    amount: u64,
    additional_signer: Option<&Keypair>,
) -> EllipsisClientResult<Signature> {
    let mut signing_keypairs = vec![&context.payer, authority];
    if let Some(signer) = additional_signer {
        signing_keypairs.push(signer);
    }

    let ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        mint,
        account,
        &authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();

    context
        .sign_send_instructions(vec![ix], signing_keypairs)
        .await
}

#[allow(dead_code)]
pub async fn get_and_bootstrap_maker(
    sdk: &mut SDKClient,
    market: &Pubkey,
    mint_authority: &Keypair,
) -> PhoenixTestAccount {
    let meta = sdk.get_market_metadata(market).await.unwrap();
    let maker = setup_account(
        &sdk.client,
        &mint_authority,
        meta.base_mint,
        meta.quote_mint,
    )
    .await;
    sdk.client.add_keypair(&maker.user);
    let mut init_instructions = vec![];
    init_instructions.push(create_claim_seat_instruction(&maker.user.pubkey(), market));
    println!("maker: {:?}", maker.user.pubkey());
    sdk.client
        .sign_send_instructions_with_payer(init_instructions, vec![&sdk.client.payer, &maker.user])
        .await
        .unwrap();

    maker
}
#[allow(dead_code)]
pub async fn get_and_bootstrap_taker(
    sdk: &mut SDKClient,
    market: &Pubkey,
    mint_authority: &Keypair,
) -> PhoenixTestAccount {
    let meta = sdk.get_market_metadata(market).await.unwrap();
    let taker = setup_account(
        &sdk.client,
        &mint_authority,
        meta.base_mint,
        meta.quote_mint,
    )
    .await;
    sdk.client.add_keypair(&taker.user);
    taker
}
