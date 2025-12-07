// --- IMPORTS ---
use kube::{Api, Client, api::{WatchEvent, WatchParams, Patch, PatchParams, PostParams}};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::batch::v1::Job;
use futures::StreamExt;
use serde_json::json;

// NEW: Metrics Imports
mod metrics;
use metrics::MetricsState;
use axum::{routing::get, Router, extract::State};
use std::net::SocketAddr;
use prometheus::{Encoder, TextEncoder};

// --- METRICS SERVER ---
// This function serves the HTTP request when Prometheus comes knocking
async fn metrics_handler(State(state): State<MetricsState>) -> String {
    let encoder = TextEncoder::new();
    
    // Gather from 'state.registry'
    let metric_families = state.registry.gather();
    
    let mut result = Vec::new();
    encoder.encode(&metric_families, &mut result).unwrap();
    String::from_utf8(result).unwrap()
}

// Spawns the server on port 8080
async fn start_metrics_server(state: MetricsState) {
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("📊 Metrics Server listening on http://{}", addr);
    
    // Start the server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// --- MAIN OPERATOR LOOP ---
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    // 1. Initialize the Observability Layer
    let metrics_state = MetricsState::new();
    
    // 2. Spawn the Web Server in the background (Non-blocking)
    let server_state = metrics_state.clone();
    tokio::spawn(async move {
        start_metrics_server(server_state).await;
    });

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");
    
    let gate_name = "kube-cache.openai.com/gate";
    let wp = WatchParams::default();
    println!("🛡️  Kube-Cache Gatekeeper Online. Waiting for Gated Pods...");

    let mut stream = pods.watch(&wp, "0").await?.boxed();

    while let Some(status) = stream.next().await {
        match status {
            Ok(WatchEvent::Added(pod)) | Ok(WatchEvent::Modified(pod)) => {
                let name = pod.metadata.name.clone().unwrap_or_default();
                
                let has_gate = pod.spec.as_ref()
                    .and_then(|s| s.scheduling_gates.as_ref())
                    .map(|gates| gates.iter().any(|g| g.name == gate_name))
                    .unwrap_or(false);

                if has_gate {
                    println!("\n🔒 LOCKED Pod Detected: {}", name);
                    
                    if let Some(annotations) = pod.metadata.annotations {
                        if let Some(data_url) = annotations.get("x-openai/required-dataset") {
                            println!("   📦 Target Data: {}", data_url);
                            println!("   ⏳ Downloading Data (Delegation)...");
                            
                            let node_target = pod.spec.as_ref()
                                .and_then(|s| s.node_name.as_deref())
                                .unwrap_or("kind-worker");

                            // Run the Fetcher Job
                            let pod_uid = pod.metadata.uid.as_deref().unwrap_or_default();
                            spawn_fetcher_job(client.clone(), data_url, node_target, &name, pod_uid).await?;
                          
                            println!("   ✅ Data Ready on Disk. Unlocking Pod...");

                            // --- RECORD THE WIN ---
                            metrics_state.record_prewarm_success(data_url);
                            // ---------------------

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
    let job_name = format!("fetcher-{}", data_url.replace("s3://","").replace("/", "-"));

    println!("   🚜 Spawning Fetcher Job {} on node {}...", job_name, node_name);

    let job_json = json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": job_name,
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
            "ttlSecondsAfterFinished": 30, 
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

    if jobs.get(&job_name).await.is_err() {
        jobs.create(&PostParams::default(), &serde_json::from_value(job_json)?).await?;
    }

    println!("   📊 Waiting for Download {} to finish...", job_name);

    loop {
        let job = jobs.get(&job_name).await?;
        if let Some(status) = job.status {
            if let Some(succeeded) = status.succeeded {
                if succeeded > 0 { break; } 
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    Ok(())
}
