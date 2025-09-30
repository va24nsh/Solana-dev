use std::{sync::Arc};
use anyhow::{Context, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::{Signer}, transaction::Transaction};
use spl_associated_token_account::{get_associated_token_address_with_program_id, instruction::create_associated_token_account};
use spl_token_client::{client::ProgramRpcClientSendTransaction, spl_token_2022::{extension::{confidential_transfer::{account_info::{TransferAccountInfo, WithdrawAccountInfo}, instruction::{configure_account, PubkeyValidityProofData}, ConfidentialTransferAccount}, BaseStateWithExtensions, ExtensionType}, instruction::reallocate, solana_zk_sdk::encryption::{auth_encryption::AeKey, elgamal::{ElGamalKeypair, ElGamalPubkey}}}, token::{ExtensionInitializationParams, Token}};
use spl_token_confidential_transfer_proof_extraction::instruction::{ProofData, ProofLocation};
use spl_token_confidential_transfer_proof_generation::withdraw::WithdrawProofData;

type TokenClient = Token<ProgramRpcClientSendTransaction>;

fn main() {
    println!("hello world");
    match load_keypair() {
        Ok(keys) => println!("{:?}", keys.pubkey()),
        Err(e) => eprintln!("Error loading keypair: {:?}", e),
    }
}

fn load_keypair() -> Result<Keypair> {
    // Get the default keypair path from the home directory
    let keypair_path = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".config/solana/id.json");

    // Read the keypair file
    let file = std::fs::File::open(&keypair_path)?;
    let keypair_bytes: Vec<u8> = serde_json::from_reader(file)?;

    // Create keypair
    let keypair = Keypair::from_bytes(&keypair_bytes)?;

    Ok(keypair)
}

async fn create_mint(token: &TokenClient, mint: &Keypair, payer: Arc<Keypair>) -> Result<()> {
    println!("Creating Mint...");

    let configuration_init_params = vec![ExtensionInitializationParams::ConfidentialTransferMint {
        authority: Some(payer.pubkey()),
        auto_approve_new_accounts: true,
        auditor_elgamal_pubkey: None,
    }];

    let transaction_signature = token.create_mint(
        &payer.pubkey(),
        Some(&payer.pubkey()),
        configuration_init_params,
        &[mint]
    ).await?;

    println!("Mint address : {}", mint.pubkey());
    println!("Signature : {}", transaction_signature);

    Ok(())
}

async fn fund_account(
    rpc_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    recipient: &Pubkey,
    amount: u64,
) -> Result<()> {
    print!("Initiating funding...");

    let fund_signature = rpc_client.send_and_confirm_transaction(
        &Transaction::new_signed_with_payer(
            &[solana_sdk::system_instruction::transfer(&payer.pubkey(), recipient, amount)], 
            Some(&payer.pubkey()), 
            &[&payer], 
            rpc_client.get_latest_blockhash().await?)
    ).await?;

    print!("Transaction signature: {}", fund_signature);
    Ok(())
}

async fn create_token_account(
    rpc_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    mint: &Keypair,
    owner: Arc<Keypair>,
    token_program_id: &Pubkey
) -> Result<(Pubkey, ElGamalKeypair, AeKey)> {
    let token_account_pubkey = get_associated_token_address_with_program_id(
        &owner.pubkey(), 
        &mint.pubkey(), 
        token_program_id,
    );

    let create_associated_token_account_ix = create_associated_token_account(
        &payer.pubkey(), 
        &owner.pubkey(), 
        &mint.pubkey(), 
        token_program_id
    );

    let reallocate_ix = reallocate(
        token_program_id, 
        &token_account_pubkey, 
        &payer.pubkey(), 
        &owner.pubkey(), 
        &[&owner.pubkey()], 
        &[ExtensionType::ConfidentialTransferAccount]
    )?;

    let elgamal_keypair = ElGamalKeypair::new_from_signer(&owner, &token_account_pubkey.to_bytes()).expect("Failed to create elgamal");
    let aes_keypair = AeKey::new_from_signer(&owner, &token_account_pubkey.to_bytes()).expect("Failed to create aes key");

    let max_pending_balance_counter = 65536;

    let decryptable_balance = aes_keypair.encrypt(0);

    let proof_data = PubkeyValidityProofData::new(&elgamal_keypair).map_err(|_| anyhow::anyhow!("Failed to generate proof data"))?;

    let proof_location = ProofLocation::InstructionOffset(1.try_into()?, ProofData::InstructionData(&proof_data));

    let configure_account_ix = configure_account(
        token_program_id, 
        &token_account_pubkey, 
        &mint.pubkey(), 
        decryptable_balance.into(), 
        max_pending_balance_counter, 
        &owner.pubkey(), 
        &[], 
        proof_location
    )?;

    let mut ixs = vec![create_associated_token_account_ix, reallocate_ix];
    ixs.extend(configure_account_ix);

    let recent_blockhash = rpc_client.get_latest_blockhash().await?;

    let transaction = Transaction::new_signed_with_payer(
        &ixs, 
        Some(&payer.pubkey()), 
        &[&payer], 
        recent_blockhash
    );

    let transaction_signature = rpc_client.send_and_confirm_transaction(&transaction).await?;

    println!("Transaction Signature for create token account: {}", transaction_signature);

    Ok((token_account_pubkey, elgamal_keypair, aes_keypair))
}

async fn mint_tokens(
    token: &TokenClient,
    token_accont: &Pubkey,
    mint_authority: &Arc<Keypair>,
    amount: u64
) -> Result<()> {
    print!("Minting tokens to {}", token_accont);

    let mint_signature = token.mint_to(
        &token_accont, 
        &mint_authority.pubkey(), 
        amount,
        &[&mint_authority]
    ).await?;

    print!("Transaction signature: {}", mint_signature);
    
    Ok(())
}

async fn deposit(
    token: &TokenClient,
    token_account: &Pubkey,
    owner: &Arc<Keypair>,
    amount: u64,
    decimals: u8
) -> Result<()> {
    println!("Depositing... ");

    let deposit_signature = token.confidential_transfer_deposit(
        &token_account, 
        &owner.pubkey(), 
        amount, 
        decimals,
        &[&owner] 
    ).await?;

    println!("Transaction Signature: {}", deposit_signature);
    Ok(())
}

async fn apply_pending_balances(
    token: &TokenClient,
    token_account: &Pubkey,
    owner: &Arc<Keypair>,
    elgamal_keypair: &ElGamalKeypair,
    aes_key: &AeKey
) -> Result<()> {
    println!("Applying Pending Balances");

    let apply_signature = token.confidential_transfer_apply_pending_balance(
        token_account, 
        &owner.pubkey(), 
        None, 
        elgamal_keypair.secret(), 
        aes_key, 
        &[&owner]
    ).await?;

    println!("Transaction Signature: {}", apply_signature);

    Ok(())
}

async fn withdraw_tokens(
    token: &TokenClient,
    token_account: &Pubkey,
    owner: &Arc<Keypair>,
    elgamal_keypair: &ElGamalKeypair,
    aes_key: &AeKey,
    amount: u64,
    decimals: u8,
    payer: Arc<Keypair>
) -> Result<()> {
    println!("Withdrawing {} from {}", amount, token_account);

    let token_account_data = token.get_account_info(token_account).await?;
    let extension_data = token_account_data.get_extension::<ConfidentialTransferAccount>()?;

    // Confidential Transfer Extension state is needed for withdraw instruction
    let withdraw_account_info = WithdrawAccountInfo::new(
        extension_data
    );

    let equality_proof_context_state_keypair = Keypair::new();
    let equality_proof_context_state_pubkey = equality_proof_context_state_keypair.pubkey();
    let range_proof_context_state_keypair = Keypair::new();
    let range_proof_context_state_pubkey = range_proof_context_state_keypair.pubkey();

    let WithdrawProofData { equality_proof_data, range_proof_data } = withdraw_account_info.generate_proof_data(amount, elgamal_keypair, aes_key)?;

    let equality_proof_signature = token.confidential_transfer_create_context_state_account(
        &equality_proof_context_state_pubkey, 
        &payer.pubkey(), 
        &equality_proof_data, 
        false, 
        &[&equality_proof_context_state_keypair]
    ).await?;

    let range_proof_signature = token.confidential_transfer_create_context_state_account(
        &range_proof_context_state_pubkey, 
        &payer.pubkey(), 
        &range_proof_data, 
        false,
        &[&range_proof_context_state_keypair] 
    ).await?;

    println!("Context state for Equality Proof {} Range proof {}", equality_proof_signature, range_proof_signature);

    let withdraw_signature = token.confidential_transfer_withdraw(
        token_account, 
        &owner.pubkey(), 
        Some(&spl_token_client::token::ProofAccount::ContextAccount(equality_proof_context_state_pubkey)), 
        Some(&spl_token_client::token::ProofAccount::ContextAccount(range_proof_context_state_pubkey)),
        amount, 
        decimals, 
        Some(withdraw_account_info), 
        elgamal_keypair, 
        aes_key, 
        &[&owner]
    ).await?;

    println!("Withdraw signature: {} ", withdraw_signature);

    let close_equality_signature = token.confidential_transfer_close_context_state_account(
        &equality_proof_context_state_pubkey, 
        token_account, 
        &payer.pubkey(),
        &[&payer]
    ).await?;

    let close_range_signature = token.confidential_transfer_close_context_state_account(
        &range_proof_context_state_pubkey, 
        token_account, 
        &payer.pubkey(), 
        &[&payer]
    ).await?;

    println!("Closed Context state for Equality Proof {} Range proof {}", close_equality_signature, close_range_signature);

    Ok(())
}

async fn transfer_token(
    token: &TokenClient,
    sender_token_account: &Pubkey,
    sender_owner: &Arc<Keypair>,
    sender_elgamal_keypair: &ElGamalKeypair,
    sender_aes_key: &AeKey,
    recipient_token_account: &Pubkey,
    recipient_elgamal_pubkey: &ElGamalPubkey,
    amount: u64,
    payer: Arc<Keypair>
) -> Result<()> {
    let token_account_data = token.get_account_info(sender_token_account).await?;
    let extension_data = token_account_data.get_extension::<ConfidentialTransferAccount>()?;
    let transfer_account_info = TransferAccountInfo::new(extension_data);

    let transfer_proof_data = transfer_account_info.generate_split_transfer_proof_data(
        amount, 
        sender_elgamal_keypair, 
        sender_aes_key, 
        recipient_elgamal_pubkey, 
        None
    )?;

    let equality_proof_context_state_keypair = Keypair::new();
    let equality_proof_context_state_pubkey = equality_proof_context_state_keypair.pubkey();

    let ciphertext_validity_proof_context_state_keypair = Keypair::new();
    let ciphertext_validity_proof_context_state_pubkey = ciphertext_validity_proof_context_state_keypair.pubkey();
    
    let range_proof_context_state_keypair = Keypair::new();
    let range_proof_context_state_pubkey = range_proof_context_state_keypair.pubkey();

    let equality_proof_signature = token.confidential_transfer_create_context_state_account(
        &equality_proof_context_state_pubkey, 
        &payer.pubkey(), 
        &transfer_proof_data.equality_proof_data, 
        false,
        &[&equality_proof_context_state_keypair]
    ).await?;

    let ciphertext_proof_signature = token.confidential_transfer_create_context_state_account(
        &ciphertext_validity_proof_context_state_pubkey, 
        &payer.pubkey(), 
        &transfer_proof_data.ciphertext_validity_proof_data, 
        false, 
        &[&ciphertext_validity_proof_context_state_keypair]
    ).await?;

    let range_proof_signature = token.confidential_transfer_create_context_state_account(
        &range_proof_context_state_pubkey, 
        &payer.pubkey(), 
        &transfer_proof_data.range_proof_data, 
        false,
        &[&range_proof_context_state_keypair] 
    ).await?;

    println!("Context Accounts Equality Proof {}, CipherText {}, Range Proof {}", equality_proof_signature, ciphertext_proof_signature, range_proof_signature);

    /**
     * All of this is off chain for now so continue later
     */
    // let ciphertext_validity_proof_account_with_ciphertext = spl_token_client::token::ProofAccountWithCiphertext {
    //     proof_account: spl_token_client::token::ProofAccount::ContextAccount(ciphertext_validity_proof_context_state_pubkey),

    // };
    
    Ok(())
}