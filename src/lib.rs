pub mod python;
pub mod config;
pub mod jupiter;
pub mod utils;

use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
    commitment_config::CommitmentLevel,
    hash::Hash,
    pubkey::Pubkey,
};
use solana_client::{
    rpc_config::RpcSendTransactionConfig,
    nonblocking::rpc_client::RpcClient,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use log::{info, warn, error, debug};
use base64::{Engine as _, engine::general_purpose};
use anyhow::{Result, Context, anyhow};
use tokio::sync::Mutex; // Required for concurrency delay

/// Struct to return structured data back to Python
#[derive(Debug, Clone)]
pub struct SwapResult {
    pub signature: String,
    pub status: String,
    pub error: Option<String>,
}

pub struct SwapEngine {
    pub rpc_primary: Arc<RpcClient>,
    pub rpc_secondary: Arc<RpcClient>,
    http_client: reqwest::Client,
    keypair: Arc<Keypair>,
    latest_blockhash: Arc<RwLock<Hash>>,
    pub user_pubkey: Pubkey,
    
    // Tracks the exact time of the last executed swap for concurrency throttling
    last_swap_time: Arc<Mutex<Instant>>,
}

impl SwapEngine {
    pub async fn new(
        helius_api_key: &str,
        backup_rpc_url: &str,
        jupiter_api_key: Option<&str>,
        keypair: Keypair,
    ) -> Result<Self> {
        info!("🚀 Initializing Swap Engine...");

        let helius_url = format!("https://mainnet.helius-rpc.com/?api-key={}", helius_api_key);
        let rpc_primary = Arc::new(RpcClient::new_with_timeout(helius_url, Duration::from_secs(10)));
        let rpc_secondary = Arc::new(RpcClient::new_with_timeout(backup_rpc_url.to_string(), Duration::from_secs(10)));

        let mut headers = HeaderMap::new();
        if let Some(key) = jupiter_api_key {
            if !key.is_empty() {
                headers.insert(HeaderName::from_static("x-api-key"), HeaderValue::from_str(key)?);
            }
        }

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(5))
            .build()?;

        let user_pubkey = keypair.pubkey();
        let keypair_arc = Arc::new(keypair);
        let latest_blockhash = Arc::new(RwLock::new(Hash::default()));

        let bh_store = latest_blockhash.clone();
        let rpc_poller = rpc_primary.clone();

        tokio::spawn(async move {
            loop {
                match rpc_poller.get_latest_blockhash().await {
                    Ok(hash) => {
                        let mut w = bh_store.write().unwrap();
                        *w = hash;
                    },
                    Err(e) => warn!("⚠️ Blockhash poll failed: {}", e),
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        });

        info!("⏳ Warming up blockhash cache...");
        let start = Instant::now();
        loop {
            if *latest_blockhash.read().unwrap() != Hash::default() {
                break;
            }
            if start.elapsed().as_secs() > 10 {
                return Err(anyhow!("Timeout waiting for initial blockhash"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("✅ Engine Ready for user: {}", user_pubkey);

        Ok(Self {
            rpc_primary,
            rpc_secondary,
            http_client,
            keypair: keypair_arc,
            latest_blockhash,
            user_pubkey,
            last_swap_time: Arc::new(Mutex::new(Instant::now())),
        })
    }

    /// Smart Swap function with Retry and Concurrency Delay Support
    pub async fn execute_swap(
        &self, 
        input_mint: &str, 
        output_mint: &str, 
        amount_lamports: u64, 
        slippage_bps: u64,
        retry: bool,
        delay_ms: u64
    ) -> SwapResult {
        
        // 1. Throttle concurrent requests
        // If 5 requests come at exactly the same time, this Mutex ensures they execute 'delay_ms' apart
        {
            let mut last_time = self.last_swap_time.lock().await;
            let elapsed = last_time.elapsed();
            let delay_duration = Duration::from_millis(delay_ms);
            
            if elapsed < delay_duration {
                tokio::time::sleep(delay_duration - elapsed).await;
            }
            *last_time = Instant::now();
        }

        let max_attempts = if retry { 3 } else { 1 };
        let mut attempt = 0;
        let mut last_error = String::new();

        // 2. Execution & Retry Loop
        while attempt < max_attempts {
            attempt += 1;
            
            match self.attempt_single_swap(input_mint, output_mint, amount_lamports, slippage_bps).await {
                Ok(signature) => {
                    return SwapResult {
                        signature,
                        status: "success".to_string(),
                        error: None,
                    };
                },
                Err(e) => {
                    last_error = e.to_string();
                    warn!("⚠️ Swap attempt {} failed: {}", attempt, last_error);
                    
                    if attempt < max_attempts {
                        // Wait before retrying
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        // If loop completes without returning, it means all attempts failed
        SwapResult {
            signature: "".to_string(),
            status: "fail".to_string(),
            error: Some(last_error),
        }
    }

    /// The actual single execution logic (Isolated for clean retry looping)
    async fn attempt_single_swap(&self, input_mint: &str, output_mint: &str, amount_lamports: u64, slippage_bps: u64) -> Result<String> {
        let start_total = Instant::now();

        let swap_tx_base64 = jupiter::get_swap_tx(
            &self.http_client,
            input_mint,
            output_mint,
            amount_lamports,
            &self.user_pubkey.to_string(),
            slippage_bps
        ).await.context("Jupiter Quote/Swap Failed")?;

        let tx_bytes = general_purpose::STANDARD.decode(&swap_tx_base64).context("Base64 Decode Failed")?;
        let mut tx: VersionedTransaction = bincode::deserialize(&tx_bytes).context("Bincode Deserialize Failed")?;

        {
            let current_hash = *self.latest_blockhash.read().unwrap();
            if current_hash == Hash::default() {
                let hash = self.rpc_primary.get_latest_blockhash().await?;
                tx.message.set_recent_blockhash(hash);
            } else {
                tx.message.set_recent_blockhash(current_hash);
            }
        }

        let signature = self.keypair.sign_message(&tx.message.serialize());
        tx.signatures = vec![signature];

        let sent_sig = self.broadcast_fast(&tx).await?;
        debug!("⚡ Total Swap Time: {:.2?}", start_total.elapsed());

        Ok(sent_sig)
    }

    async fn broadcast_fast(&self, tx: &VersionedTransaction) -> Result<String> {
        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: Some(CommitmentLevel::Processed),
            ..RpcSendTransactionConfig::default()
        };

        let tx_clone = tx.clone();
        let config_clone = config.clone();

        let (res1, res2) = tokio::join!(
            self.rpc_primary.send_transaction_with_config(tx, config),
            self.rpc_secondary.send_transaction_with_config(&tx_clone, config_clone)
        );

        match (res1, res2) {
            (Ok(sig), _) => Ok(sig.to_string()),
            (_, Ok(sig)) => Ok(sig.to_string()),
            (Err(e1), Err(e2)) => Err(anyhow!("Broadcast Failed. Primary: {}, Backup: {}", e1, e2))
        }
    }
}