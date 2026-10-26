use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, native_token::LAMPORTS_PER_SOL};
use spl_associated_token_account::get_associated_token_address;
use anyhow::{Result, anyhow};

/// --- 1. UNIT CONVERSION ---

/// Converts SOL amount to Lamports (1 SOL = 1,000,000,000 Lamports).
pub fn sol_to_lamports(amount: f64) -> u64 {
    (amount * LAMPORTS_PER_SOL as f64) as u64
}

/// Converts Lamports to SOL.
pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / LAMPORTS_PER_SOL as f64
}

/// Converts any Token amount to its Raw Amount based on decimals.
/// Example: 100 USDC (6 decimals) -> 100_000_000
pub fn token_ui_amount_to_raw(amount: f64, decimals: u8) -> u64 {
    (amount * 10u64.pow(decimals as u32) as f64) as u64
}

/// --- 2. ON-CHAIN DATA HELPERS ---

/// Fetches the current SOL balance of a wallet.
pub async fn get_sol_balance(rpc: &RpcClient, pubkey: &Pubkey) -> Result<f64> {
    match rpc.get_balance(pubkey).await {
        Ok(bal) => Ok(lamports_to_sol(bal)),
        Err(e) => Err(anyhow!("Failed to fetch SOL balance: {}", e)),
    }
}

/// Fetches the Token Balance of a specific Mint for a Wallet.
/// Handles the case where the Token Account (ATA) does not exist yet (returns 0.0).
pub async fn get_token_balance(rpc: &RpcClient, wallet: &Pubkey, mint: &Pubkey) -> Result<f64> {
    // 1. Derive the ATA (Associated Token Account) address
    let ata_address = get_associated_token_address(wallet, mint);

    // 2. Fetch Balance
    match rpc.get_token_account_balance(&ata_address).await {
        Ok(ui_amount) => {
            // If amount exists, parse it, otherwise 0.0
            Ok(ui_amount.ui_amount.unwrap_or(0.0))
        },
        Err(_) => {
            // Usually errors if the account doesn't exist. We treat this as 0 balance.
            Ok(0.0)
        }
    }
}

/// Checks if a Token Account exists for a wallet.
pub async fn token_account_exists(rpc: &RpcClient, wallet: &Pubkey, mint: &Pubkey) -> bool {
    let ata = get_associated_token_address(wallet, mint);
    rpc.get_account(&ata).await.is_ok()
}

/// --- 3. LOGGING & DEBUGGING ---

/// Returns a clickable Solscan Transaction Link.
pub fn get_solscan_tx_url(signature: &str) -> String {
    format!("https://solscan.io/tx/{}", signature)
}

/// Returns a clickable Solscan Account Link.
pub fn get_solscan_account_url(pubkey: &str) -> String {
    format!("https://solscan.io/account/{}", pubkey)
}

/// --- 4. RETRY LOGIC (Optional but Recommended) ---

/// A simple async retry wrapper.
/// Retries a function `max_retries` times with a delay.
pub async fn retry<F, Fut, T>(
    mut operation: F,
    max_retries: u32,
    delay_ms: u64
) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt > max_retries {
                    return Err(e);
                }
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
}