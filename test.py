import asyncio
import os
import sys

# Optional: Load environment variables from .env file if python-dotenv is installed
try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass

from solana_flash_swap import PySwapEngine


async def main():
    # -------------------------------------------------------------------------
    # 1. Configuration Setup
    # -------------------------------------------------------------------------
    # Load parameters from environment variables or provide custom values
    helius_api_key = os.getenv("HELIUS_API_KEY", "your_helius_api_key_here")
    backup_rpc_url = os.getenv("BACKUP_RPC_URL", "https://api.mainnet-beta.solana.com")
    jupiter_api_key = os.getenv("JUPITER_API_KEY", None)
    wallet_path = os.getenv("WALLET_PATH", "id.json")

    # Validate that dummy credentials have been replaced before execution
    if helius_api_key == "your_helius_api_key_here" or not os.path.exists(wallet_path):
        print("⚠️  Warning: Missing credentials or wallet file.")
        print("   Please create a `.env` file from `.env.example` or set HELIUS_API_KEY and WALLET_PATH.")
        print(f"   Current wallet path: {wallet_path} (exists: {os.path.exists(wallet_path)})")
        print("\nExiting to prevent failed execution.")
        sys.exit(1)

    # -------------------------------------------------------------------------
    # 2. Token Mint Addresses (Solana Mainnet)
    # -------------------------------------------------------------------------
    USDT_MINT = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"
    SOL_MINT = "So11111111111111111111111111111111111111112"

    # -------------------------------------------------------------------------
    # 3. Swap Parameters
    # -------------------------------------------------------------------------
    # USDT has 6 decimals: 0.01 USDT = 0.01 * 10^6 = 10,000 raw units
    amount_raw = 10_000

    # Slippage tolerance in basis points (50 bps = 0.5%)
    slippage_bps = 50

    try:
        print("🚀 Initializing Solana Flash Swap Engine...")
        engine = PySwapEngine(
            helius_api_key,
            backup_rpc_url,
            jupiter_api_key,
            wallet_path
        )

        user_pubkey = engine.get_user_pubkey()
        print(f"✅ Wallet Connected: {user_pubkey}")

        print("🔄 Executing swap: 0.01 USDT -> SOL ...")

        # Execute the swap asynchronously via Rust core
        # Optional keyword args: retry=True (3 attempts), delay_ms=500
        result = await engine.execute_swap(
            USDT_MINT,
            SOL_MINT,
            amount_raw,
            slippage_bps,
            retry=True,
            delay_ms=500
        )

        # PySwapEngine returns a dictionary: {'signature': str, 'status': str, 'error': Optional[str]}
        if result.get("status") == "success":
            signature = result.get("signature")
            print(f"🎉 Swap Succeeded! Transaction Signature: {signature}")
            print(f"🔍 Solscan Link: https://solscan.io/tx/{signature}")
        else:
            error_message = result.get("error", "Unknown error")
            print(f"❌ Swap Failed: {error_message}")

    except Exception as exc:
        print(f"❌ Execution Error: {exc}")


if __name__ == "__main__":
    asyncio.run(main())
