use bdk_esplora::esplora_client;

fn main() -> Result<(), anyhow::Error> {
    // Build a blocking Esplora client with a custom overall request timeout
    // Note: requires `bdk_esplora` with `blocking-https` feature (already enabled in dev-deps)
    let client = esplora_client::Builder::new("https://blockstream.info/api")
        // Timeout is specified in milliseconds for the unified API
        .timeout(30_000)
        .build_blocking();

    // Make a simple request and handle timeout-related errors gracefully
    match client.get_height() {
        Ok(height) => println!("Current blockchain height: {}", height),
        Err(err) => {
            eprintln!("Request failed: {err}");
            // Application-specific handling could inspect `err` for timeouts and retry
        }
    }

    Ok(())
}


