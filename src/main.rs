// --- IMPORTS ---
// We are bringing in specific tools from the libraries we installed.
use kube::{Api, Client, api::{ListParams, WatchEvent}};
use futures::StreamExt;


use k8s_openapi::api::core::v1::Pod;


// --- THE MAIN FUNCTION ---
// The attribute #[tokio::main] is a "Macro".
// Rust's default 'main' function cannot be asynchronous (it can't wait for network requests).
// This macro wraps our function in a runtime that CAN wait for network requests.
#[tokio::main]

async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    // 1. Initialize Logging
    // This turns on the printing tools so we can see output in the terminal.
    tracing_subscriber::fmt::init();

    println!("🚀 Kube-Cache Operator is starting...");

    // 2. Connect to Kubernetes
    // 'let' defines a variable.
    // 'Client::try_default()' tries to find your ~/.kube/config file.
    // '.await' means: "Pause this line until the connection is ready."
    // '?' means: "If this FAILS, stop the program and print the error. If it SUCCEEDS, give me the client."
    let client = Client::try_default().await?;

    // 3. Define the API we want to talk to
    // We want to talk to 'Pods' in the 'kube-system' namespace.
    // The syntax '<Pod>' is a Generic. It tells the compiler: "This API expects Pod objects, not Services or Nodes."
    let pods: Api<Pod> = Api::namespaced(client, "kube-system");

    // ListParams::default() means "Give me everything, no filters yet."
    let lp = ListParams::default();
    
    println!("✅ Connection successful! Listing system pods:");

    // 4. The Loop
    // pods.list(&lp).await? -> Fetch the list from the server (pause until done).
    // 'for p in ...' -> Iterate over every pod found.
    for p in pods.list(&lp).await? {
        // p.metadata.name is an "Option". It might be something (Some) or nothing (None).
        // .unwrap_or_default() says: "If the name is missing, just give me an empty string. Don't crash."
        println!("   - Found Pod: {}", p.metadata.name.unwrap_or_default());
    }

    // Return "Ok". 
    // In Rust, the last line of a function (without a semicolon) is the return value.
    // () is called a "Unit", basically meaning "void" or "nothing useful, but not an error."
    Ok(())
}