use serde::Deserialize;
use anyhow::{Result, Context};
use std::env;

/// Global constants for default values
pub const DEFAULT_SLIPPAGE_BPS: u64 = 50; // 0.5%
pub const DEFAULT_RPC_TIMEOUT_SECS: u64 = 10;
pub const DEFAULT_TX_RETRIES: u32 = 3;

/// Represents the configuration for the Trading Engine.
/// This can be loaded from environment variables or a config file.
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// The primary Helius RPC URL (e.g., https://mainnet.helius-rpc.com/...)
    pub helius_api_key: String,

    /// A backup RPC URL (e.g., standard mainnet beta)
    pub backup_rpc_url: String,

    /// Optional Jupiter API Key (for higher rate limits)
    pub jupiter_api_key: Option<String>,

    /// Path to the wallet keypair file (e.g., "my-wallet.json")
    pub wallet_path: String,

    /// Default slippage tolerance in basis points (100 = 1%)
    pub default_slippage_bps: u64,
}

impl AppConfig {
    /// Loads configuration from Environment Variables.
    ///
    /// # Expected Env Vars:
    /// - `HELIUS_API_KEY`
    /// - `BACKUP_RPC_URL` (optional, defaults to public mainnet)
    /// - `JUPITER_API_KEY` (optional)
    /// - `WALLET_PATH` (optional, defaults to "id.json")
    /// - `SLIPPAGE_BPS` (optional, defaults to 50)
    pub fn from_env() -> Result<Self> {
        // Load .env file if it exists (requires 'dotenv' crate if used, but we stick to std::env here)
        // Note: Call `dotenv::dotenv().ok()` in your main.rs before calling this if using .env files.

        let helius_api_key = env::var("HELIUS_API_KEY")
            .context("❌ HELIUS_API_KEY must be set in environment variables")?;

        let backup_rpc_url = env::var("BACKUP_RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());

        let jupiter_api_key = env::var("JUPITER_API_KEY").ok();

        let wallet_path = env::var("WALLET_PATH")
            .unwrap_or_else(|_| "id.json".to_string());

        let default_slippage_bps = env::var("SLIPPAGE_BPS")
            .unwrap_or_else(|_| DEFAULT_SLIPPAGE_BPS.to_string())
            .parse::<u64>()
            .unwrap_or(DEFAULT_SLIPPAGE_BPS);

        Ok(Self {
            helius_api_key,
            backup_rpc_url,
            jupiter_api_key,
            wallet_path,
            default_slippage_bps,
        })
    }

    /// Helper to get the full Helius URL with API key
    pub fn get_helius_url(&self) -> String {
        format!("https://mainnet.helius-rpc.com/?api-key={}", self.helius_api_key)
    }
}