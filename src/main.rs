use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

const DEFAULT_RPC: &str = "https://api.mainnet-beta.solana.com";
const DEFAULT_USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"; // official mainnet USDC mint

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| DEFAULT_RPC.to_string());
    let usdc_mint = env::var("USDC_MINT").unwrap_or_else(|_| DEFAULT_USDC_MINT.to_string());

    println!("RPC: {}", rpc_url);
    println!("USDC mint: {}", usdc_mint);

    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(15))
        .build()?;

    // track the latest signature we've processed to avoid duplicates
    let mut last_seen: Option<String> = None;

    loop {
        match poll_once(&client, &rpc_url, &usdc_mint, &last_seen).await {
            Ok(new_latest) => {
                if let Some(sig) = new_latest {
                    last_seen = Some(sig);
                }
            }
            Err(e) => eprintln!("poll error: {}", e),
        }

        // sleep before next poll
        sleep(Duration::from_secs(10)).await;
    }
}

async fn poll_once(
    client: &Client,
    rpc_url: &str,
    mint: &str,
    last_seen: &Option<String>,
) -> Result<Option<String>> {
    // 1) getSignaturesForAddress (most recent first)
    let params = vec![serde_json::json!(mint), serde_json::json!({ "limit": 20 })];

    let sigs = rpc_request(client, rpc_url, "getSignaturesForAddress", params).await?;

    let arr = sigs.as_array().context("expected array of signatures")?;

    if arr.is_empty() {
        return Ok(None);
    }

    // find newest signature greater than last_seen
    // arr is ordered newest -> oldest, so we'll iterate and collect new ones
    let mut new_sigs: Vec<String> = vec![];

    for entry in arr.iter() {
        if let Some(sig) = entry.get("signature").and_then(|v| v.as_str()) {
            if Some(sig.to_string()) == *last_seen {
                break; // we've already processed older ones
            }
            new_sigs.push(sig.to_string());
        }
    }

    if new_sigs.is_empty() {
        return Ok(None);
    }

    // process from oldest -> newest
    new_sigs.reverse();

    let mut newest_processed: Option<String> = None;

    for sig in new_sigs.iter() {
        match fetch_and_handle_tx(client, rpc_url, sig).await {
            Ok(found_burn) => {
                if found_burn {
                    println!("[{}] detected burn in tx {}", now_ts()?, sig);
                }
                newest_processed = Some(sig.clone());
            }
            Err(e) => eprintln!("error processing {}: {}", sig, e),
        }
    }

    Ok(newest_processed)
}

async fn fetch_and_handle_tx(client: &Client, rpc_url: &str, signature: &str) -> Result<bool> {
    let params = vec![
        serde_json::json!(signature),
        serde_json::json!({
            "encoding": "jsonParsed",
            "maxSupportedTransactionVersion": 0
        }),
    ];

    let resp = rpc_request(client, rpc_url, "getTransaction", params).await?;

    if resp.is_null() {
        // transaction might not be available (yet)
        return Ok(false);
    }

    // parsed transaction structure: resp.transaction.message.instructions
    // We'll search for any instruction where program == "spl-token" and parsed.type == "burn"
    if let Some(tx) = resp.get("transaction") {
        if let Some(message) = tx.get("message") {
            if let Some(instructions) = message.get("instructions").and_then(|v| v.as_array()) {
                for instr in instructions.iter() {
                    // check program
                    let program = instr.get("program").and_then(|v| v.as_str()).unwrap_or("");
                    if program == "spl-token" {
                        // parsed may be present
                        if let Some(parsed) = instr.get("parsed") {
                            if let Some(instr_type) = parsed.get("type").and_then(|v| v.as_str()) {
                                if instr_type.eq_ignore_ascii_case("burn") {
                                    // pull details
                                    let info = parsed.get("info").unwrap_or(&Value::Null);
                                    let amount =
                                        info.get("amount").and_then(|v| v.as_str()).unwrap_or("?");
                                    let source =
                                        info.get("source").and_then(|v| v.as_str()).unwrap_or("?");
                                    let mint =
                                        info.get("mint").and_then(|v| v.as_str()).unwrap_or("?");

                                    println!(
                                        "BURN detected: tx={} mint={} source={} amount={}",
                                        signature, mint, source, amount
                                    );
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // additionally, some burn instructions may be inside innerInstructions in meta
    if let Some(meta) = resp.get("meta") {
        if let Some(inner) = meta.get("innerInstructions") {
            if let Some(inner_array) = inner.as_array() {
                for inner_grp in inner_array.iter() {
                    if let Some(instrs) = inner_grp.get("instructions").and_then(|v| v.as_array()) {
                        for instr in instrs.iter() {
                            let program =
                                instr.get("program").and_then(|v| v.as_str()).unwrap_or("");
                            if program == "spl-token" {
                                if let Some(parsed) = instr.get("parsed") {
                                    if let Some(instr_type) =
                                        parsed.get("type").and_then(|v| v.as_str())
                                    {
                                        if instr_type.eq_ignore_ascii_case("burn") {
                                            let info = parsed.get("info").unwrap_or(&Value::Null);
                                            let amount = info
                                                .get("amount")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("?");
                                            let source = info
                                                .get("source")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("?");
                                            let mint = info
                                                .get("mint")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("?");

                                            println!(
                                                "BURN (inner) detected: tx={} mint={} source={} amount={}",
                                                signature, mint, source, amount
                                            );
                                            return Ok(true);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

async fn rpc_request(
    client: &Client,
    url: &str,
    method: &str,
    params: Vec<Value>,
) -> Result<Value> {
    let req_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let res = client.post(url).json(&req_body).send().await?;

    let status = res.status();
    let text = res.text().await?;

    if !status.is_success() {
        bail!("RPC error {}: {}", status, text);
    }

    let v: Value = serde_json::from_str(&text)?;
    if let Some(err) = v.get("error") {
        bail!("rpc error: {}", err);
    }

    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

fn now_ts() -> Result<u64> {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(dur.as_secs())
}
