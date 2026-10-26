use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::PyDict;

use pyo3_asyncio::tokio::future_into_py;
use std::sync::Arc;

use solana_sdk::signature::read_keypair_file;

use crate::SwapEngine;

#[pyclass]
pub struct PySwapEngine {
    inner: Arc<SwapEngine>,
}

#[pymethods]
impl PySwapEngine {

    #[new]
    #[pyo3(signature = (helius_api_key, backup_rpc_url, jupiter_api_key, wallet_path))]
    fn new(
        helius_api_key: String,
        backup_rpc_url: String,
        jupiter_api_key: Option<String>,
        wallet_path: String,
    ) -> PyResult<Self> {
        let runtime = pyo3_asyncio::tokio::get_runtime();

        let keypair = read_keypair_file(&wallet_path)
            .map_err(|e| PyRuntimeError::new_err(format!("Wallet load failed: {}", e)))?;

        let engine = runtime.block_on(async {
            crate::SwapEngine::new(
                &helius_api_key,
                &backup_rpc_url,
                jupiter_api_key.as_deref(),
                keypair,
            ).await
        }).map_err(|e| PyRuntimeError::new_err(format!("Engine init failed: {}", e)))?;

        Ok(Self {
            inner: Arc::new(engine),
        })
    }

    /// Execute swap async with optional retry and delay
    /// Default: retry = false, delay_ms = 500
    #[pyo3(signature = (input_mint, output_mint, amount_lamports, slippage_bps, retry=false, delay_ms=500))]
    fn execute_swap<'py>(
        &'py self,
        py: Python<'py>,
        input_mint: String,
        output_mint: String,
        amount_lamports: u64,
        slippage_bps: u64,
        retry: bool,
        delay_ms: u64,
    ) -> PyResult<&'py PyAny> {

        let engine = self.inner.clone();

        // Run the async rust logic
        future_into_py(py, async move {
            let result = engine.execute_swap(
                &input_mint,
                &output_mint,
                amount_lamports,
                slippage_bps,
                retry,
                delay_ms
            ).await;

            // Convert Rust's SwapResult into a Python Dictionary
            Python::with_gil(|py| {
                let dict = PyDict::new(py);
                dict.set_item("signature", result.signature).unwrap();
                dict.set_item("status", result.status).unwrap();
                
                match result.error {
                    Some(err) => dict.set_item("error", err).unwrap(),
                    None => dict.set_item("error", py.None()).unwrap(),
                }

                Ok(dict.to_object(py))
            })
        })
    }

    fn get_user_pubkey(&self) -> PyResult<String> {
        Ok(self.inner.user_pubkey.to_string())
    }
}

#[pymodule]
fn solana_flash_swap(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PySwapEngine>()?;
    Ok(())
}