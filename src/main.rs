// --- IMPORTS ---
// We are bringing in specific tools from the libraries we installed.
use kube::{Api, Client, api::{WatchEvent, WatchParams, Patch, PatchParams}};
use futures::StreamExt;
use serde_json::json; // for patching json.


use k8s_openapi::api::core::v1::Pod;


// --- THE MAIN FUNCTION ---
// The attribute #[tokio::main] is a "Macro".
// Rust's default 'main' function cannot be asynchronous (it can't wait for network requests).
// This macro wraps our function in a runtime that CAN wait for network requests.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client, "default");
    
    // The "Lock" name we are looking for
    let gate_name = "kube-cache.openai.com/gate";

    let wp = WatchParams::default();
    println!("🛡️  Kube-Cache Gatekeeper Online. Waiting for Gated Pods...");

    let mut stream = pods.watch(&wp, "0").await?.boxed();

    while let Some(status) = stream.next().await {
        match status {
            Ok(WatchEvent::Added(pod)) | Ok(WatchEvent::Modified(pod)) => {
                let name = pod.metadata.name.clone().unwrap_or_default();
                
                // 1. Check if the Pod is "Gated" (Waiting for us)
                // We look inside pod.spec.scheduling_gates
                let has_gate = pod.spec.as_ref()
                    .and_then(|s| s.scheduling_gates.as_ref())
                    .map(|gates| gates.iter().any(|g| g.name == gate_name))
                    .unwrap_or(false);

                if has_gate {
                    println!("\n🔒 LOCKED Pod Detected: {}", name);
                    
                    // 2. Check WHICH data it needs
                    if let Some(annotations) = pod.metadata.annotations {
                        if let Some(data_url) = annotations.get("x-openai/required-dataset") {
                            println!("   📦 Target Data: {}", data_url);
                            println!("   ⏳ Downloading Data (Simulation)...");
                            
                            // SIMULATION: Wait 2 seconds to pretend we are downloading 500GB
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            
                            println!("   ✅ Download Complete. Unlocking Pod...");

                            // 3. THE ACTION: Remove the Gate
                            // We send a JSON Patch to delete the scheduling gate
                            let patch = json!({
                                "spec": {
                                    "schedulingGates": [] 
                                }
                            });
                            
                            let pp = PatchParams::default();
                            pods.patch(&name, &pp, &Patch::Merge(patch)).await?;
                            
                            println!("   🚀 Pod '{}' Released to Scheduler!", name);
                        }
                    }
                }
            },
            Ok(WatchEvent::Error(e)) => println!("Error: {}", e),
            _ => {}
        }
    }

    Ok(())
}