// --- IMPORTS ---
// We are bringing in specific tools from the libraries we installed.
use kube::{Api, Client, api::{WatchEvent, WatchParams, Patch, PatchParams,PostParams}};

use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::batch::v1::Job; // <--- NEW: Import Job struct
use futures::StreamExt;
use serde_json::json;




// --- THE MAIN FUNCTION ---
// The attribute #[tokio::main] is a "Macro".
// Rust's default 'main' function cannot be asynchronous (it can't wait for network requests).
// This macro wraps our function in a runtime that CAN wait for network requests.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");
    
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
                            
                            //2.5 determine WHERE to run the job.
                            // we need to run the fetcher on the same node as the gpu pod.
                            // if the pod hasnt been assigned to a node yet we cant pre-warm.
                            // (Note: In a real scheduler, we would pick the node here. 
                            // For this demo, we assume the pod is assigned or we default to 'worker')
                            let node_target = pod.spec.as_ref()
                            .and_then(|s| s.node_name.as_deref())
                            .unwrap_or("kind-worker"); // default fall back for local testing

                            // run the real job
                            let pod_uid = pod.metadata.uid.as_deref().unwrap_or_default();
                            spawn_fetcher_job(client.clone(), data_url, node_target, &name, pod_uid).await?;
                          
                            println!("   ✅ Data Ready on Disk. Unlocking Pod...");

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


async fn spawn_fetcher_job(client: Client, data_url: &str, node_name: &str, pod_name: &str, pod_uid: &str) -> Result<(), Box<dyn std::error::Error>> {
    let jobs: Api<Job> = Api::namespaced(client, "default");
    // create unique name for the job
    // we replace special characters to make valid k8s names
    let job_name = format!("fetcher-{}", data_url.replace("s3://","").replace("/", "-"));

    println!(" 🚜 Spawning Fetcher Job {} on node {}...", job_name, node_name);

    // define the job using JSON (syntax looks like YAML)


    // Define the Job using JSON (It looks just like YAML!)
    let job_json = json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": job_name,
            // THIS IS THE GARBAGE COLLECTOR MAGIC
            "ownerReferences": [{
                "apiVersion": "v1",
                "kind": "Pod",
                "name": pod_name,
                "uid": pod_uid,
                "controller": true,
                "blockOwnerDeletion": true
            }]
        },
        "spec": {
            "ttlSecondsAfterFinished": 30, // Keep this! It cleans up successful jobs.
            "template": {
                "spec": {
                    "nodeName": node_name,
                    "restartPolicy": "Never",
                    "containers": [{
                        "name": "downloader",
                        "image": "alpine",
                        "command": ["sh", "-c", "echo 'Downloading specific dataset...'; sleep 5; echo 'Done!'"]
                    }]
                }
            }
        }
    });

    // send it to kubernetes
    
    jobs.create(&PostParams::default(), &serde_json::from_value(job_json)?).await?;


    // Wait for it to finish (Simple Polling)
    println!(" 📊 Waiting for Download {} to finish...", job_name);

    loop {
        let job = jobs.get(&job_name).await?;
        if let Some(status) = job.status {
            if let Some(succeeded) = status.succeeded{
                if succeeded > 0 { break; } // Success
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    }

    Ok(())

}