use reqwest::Client;
use serde_json::{json, Value};
use anyhow::{Result, anyhow, Context};
use log::{debug, error};

// Jupiter API Endpoints
const JUPITER_QUOTE_API: &str = "https://api.jup.ag/swap/v1/quote";
const JUPITER_SWAP_API: &str = "https://api.jup.ag/swap/v1/swap";

/// Fetches a quote and builds a signed-ready swap transaction from Jupiter.
///
/// # Arguments
/// * `client` - The reused HTTP client (for keep-alive connections).
/// * `input_mint` - Source Token Mint Address.
/// * `output_mint` - Destination Token Mint Address.
/// * `amount` - Amount in raw lamports/decimals (e.g., 1 SOL = 1000000000).
/// * `pubkey` - The wallet address that will sign the transaction.
/// * `slippage_bps` - Slippage tolerance in Basis Points (e.g., 50 = 0.5%, 100 = 1%).
///
/// # Returns
/// * `Result<String>` - Base64 encoded transaction string ready to be signed.
pub async fn get_swap_tx(
    client: &Client,
    input_mint: &str,
    output_mint: &str,
    amount: u64,
    pubkey: &str,
    slippage_bps: u64
) -> Result<String> {

    // --- STEP 1: GET QUOTE ---
    // We request the best route.
    // 'onlyDirectRoutes' is omitted to ensure we find a route even if it's complex,
    // but you can add `&onlyDirectRoutes=true` for slightly faster calculation at the cost of price.
    let quote_url = format!(
        "{}?inputMint={}&outputMint={}&amount={}&slippageBps={}",
        JUPITER_QUOTE_API, input_mint, output_mint, amount, slippage_bps
    );

    debug!("Fetching Quote from: {}", quote_url);

    let quote_res = client.get(&quote_url)
        .send()
        .await
        .context("Failed to send Quote request")?;

    // Check HTTP Status
    if !quote_res.status().is_success() {
        let error_text = quote_res.text().await.unwrap_or_default();
        error!("Jupiter Quote Failed: {}", error_text);
        return Err(anyhow!("Quote API Error: {}", error_text));
    }

    let quote_json: Value = quote_res.json().await.context("Failed to parse Quote JSON")?;

    // Check if Jupiter returned an internal error inside JSON
    if let Some(err) = quote_json.get("error") {
        return Err(anyhow!("Jupiter returned error: {}", err));
    }

    // --- STEP 2: BUILD SWAP TRANSACTION ---
    // We construct the payload for the swap endpoint.
    // 'dynamicComputeUnitLimit': true -> Crucial for complex routing (prevents CU exceeded errors).
    // 'prioritizationFeeLamports': 'auto' -> Jupiter calculates the necessary fee to land the Tx fast.
    let swap_req = json!({
        "quoteResponse": quote_json,
        "userPublicKey": pubkey,
        "wrapAndUnwrapSol": true,
        "dynamicComputeUnitLimit": true,
        "prioritizationFeeLamports": "auto"
    });

    debug!("Building Swap Transaction...");

    let swap_res = client.post(JUPITER_SWAP_API)
        .json(&swap_req)
        .send()
        .await
        .context("Failed to send Swap Build request")?;

    if !swap_res.status().is_success() {
        let error_text = swap_res.text().await.unwrap_or_default();
        error!("Jupiter Swap Build Failed: {}", error_text);
        return Err(anyhow!("Swap API Error: {}", error_text));
    }

    let swap_body: Value = swap_res.json().await.context("Failed to parse Swap JSON")?;

    // --- STEP 3: EXTRACT TRANSACTION ---
    match swap_body.get("swapTransaction") {
        Some(tx_value) => {
            let tx_str = tx_value.as_str()
                .ok_or_else(|| anyhow!("swapTransaction is not a string"))?;
            Ok(tx_str.to_string())
        },
        None => {
            error!("Response missing 'swapTransaction'. Full Body: {:?}", swap_body);
            Err(anyhow!("Jupiter response missing swapTransaction data"))
        }
    }
}