# USDC Burn Listener

A Rust application that monitors the Solana blockchain for USDC token burn events in real-time.

## Features

- Polls Solana RPC for new transactions on the USDC mint address
- Detects SPL token burn instructions in transactions
- Tracks processed signatures to avoid duplicates
- Configurable RPC endpoint and mint address via environment variables

## Usage

```bash
# Run with default settings (mainnet USDC)
cargo run

# Use custom RPC endpoint
RPC_URL=https://your-rpc-endpoint.com cargo run

# Monitor different mint address
USDC_MINT=YourMintAddressHere cargo run
```

## Environment Variables

- `RPC_URL`: Solana RPC endpoint (default: `https://api.mainnet-beta.solana.com`)
- `USDC_MINT`: Token mint address to monitor (default: `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`)

## Output

When a burn event is detected, the application outputs:
```
BURN detected: tx=<signature> mint=<mint_address> source=<source_account> amount=<amount>
```

## Dependencies

- `tokio`: Async runtime
- `reqwest`: HTTP client for RPC calls
- `serde_json`: JSON parsing
- `anyhow`: Error handling
- `log`/`env_logger`: Logging
