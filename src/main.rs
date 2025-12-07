// --- IMPORTS ---
use kube::{Api, Client, api::{WatchEvent, WatchParams, Patch, PatchParams, PostParams}};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::batch::v1::Job;
use futures::StreamExt;
use serde_json::json;

// NEW: Logging Imports
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

// NEW: Metrics Imports
mod metrics;
use metrics::MetricsState;
use axum::{routing::get, Router, extract::State};
use std::net::SocketAddr;
use prometheus::{Encoder, TextEncoder};

// --- METRICS SERVER ---
async fn metrics_handler(State(state): State<MetricsState>) -> String {
    let encoder = TextEncoder::new();
    let metric_families = state.registry.gather();
    let mut result = Vec::new();
    encoder.encode(&metric_families, &mut result).unwrap();
    String::from_utf8(result).unwrap()
}

async fn start_metrics_server(state: MetricsState) {
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    
    // LOG 1: Server Start (Structured)
    info!(event = "server_start", port = 8080, "Metrics Server listening");
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// --- MAIN OPERATOR LOOP ---
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. INITIALIZE JSON LOGGING (The "Black Box" Recorder)
    let subscriber = FmtSubscriber::builder()
        .json()                 // Output as JSON
        .with_max_level(Level::INFO)
        .with_current_span(false)
        .with_file(true)        // Log which file the message came from
        .with_line_number(true) // Log the exact line number
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");
    
    // 2. Initialize the Observability Layer
    let metrics_state = MetricsState::new();
    
    // 3. Spawn the Web Server
    let server_state = metrics_state.clone();
    tokio::spawn(async move {
        start_metrics_server(server_state).await;
    });

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");
    
    let gate_name = "kube-cache.openai.com/gate";
    let wp = WatchParams::default();

    // LOG 2: Operator Online
    info!(event = "startup", version = env!("CARGO_PKG_VERSION"), "Kube-Cache Gatekeeper Online");

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
                    // LOG 3: Detection
                    info!(event = "pod_locked", pod_name = %name, "Locked Pod Detected");
                    
                    if let Some(annotations) = pod.metadata.annotations {
                        if let Some(data_url) = annotations.get("x-openai/required-dataset") {
                            
                            // LOG 4: Delegation Start
                            info!(event = "delegation_start", pod_name = %name, dataset = %data_url, "Delegating download to job");
                            
                            let node_target = pod.spec.as_ref()
                                .and_then(|s| s.node_name.as_deref())
                                .unwrap_or("kind-worker");

    
                            // Run the Fetcher Job
                            let pod_uid = pod.metadata.uid.as_deref().unwrap_or_default();
                            
                            // 1. Construct a file path to check (Simulation Logic)
                            // We treat the filename as the "cache key"
                            let filename = data_url.replace("s3://", "").replace("/", "-");
                            let file_path = format!("/tmp/{}", filename);

                            // 2. The Logic: Hit vs Miss
                            if std::path::Path::new(&file_path).exists() {
                                // --- CACHE HIT ---
                                info!(event = "cache_hit", pod_name = %name, path = %file_path, "Dataset found locally");
                                metrics_state.count_hit();
                                
                            } else {
                                // --- CACHE MISS ---
                                info!(event = "cache_miss", pod_name = %name, path = %file_path, "Downloading dataset");
                                metrics_state.count_miss();

                                // A. Start Timer
                                let start = std::time::Instant::now();

                                // B. Do the "Download" (Spawn the K8s Job)
                                spawn_fetcher_job(client.clone(), data_url, node_target, &name, pod_uid).await?;

                                // C. Stop Timer & Record
                                let duration = start.elapsed().as_secs_f64();
                                metrics_state.observe_warmup(duration);

                                // D. Create the dummy file so next time it counts as a HIT (Simulation)
                                if let Ok(_) = std::fs::File::create(&file_path) {
                                    info!(event = "cache_update", path = %file_path, "Cache updated on disk");
                                }
                            }

                            // LOG 5: Data Ready
                            info!(event = "data_ready", pod_name = %name, "Data ready on disk");
                                

                            let patch = json!({
                                "spec": {
                                    "schedulingGates": [] 
                                }
                            });
                            
                            let pp = PatchParams::default();
                            pods.patch(&name, &pp, &Patch::Merge(patch)).await?;
                            
                            // LOG 6: Release
                            info!(event = "pod_release", pod_name = %name, "Pod released to scheduler");
                        }
                    }
                }
            },
            Ok(WatchEvent::Error(e)) => error!(error = ?e, "Watch stream error"),
            _ => {}
        }
    }

    Ok(())
}

async fn spawn_fetcher_job(client: Client, data_url: &str, node_name: &str, pod_name: &str, pod_uid: &str) -> Result<(), Box<dyn std::error::Error>> {
    let jobs: Api<Job> = Api::namespaced(client, "default");
    let job_name = format!("fetcher-{}", data_url.replace("s3://","").replace("/", "-"));

    // LOG 7: Spawning Job
    info!(event = "job_spawn", job_name = %job_name, node = %node_name, "Spawning fetcher job");

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

    // LOG 8: Waiting
    info!(event = "job_wait", job_name = %job_name, "Waiting for download to finish");

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