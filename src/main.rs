// --- IMPORTS ---
use kube::{Api, Client, api::{WatchEvent, WatchParams, Patch, PatchParams}};
use k8s_openapi::api::core::v1::Pod;
// use k8s_openapi::api::batch::v1::Job; // Removed unused import
use futures::StreamExt;
use serde_json::json;

// --- NEW IMPORTS FOR S3 ---
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{Client as S3Client, config::Region};
use std::fs::File;
use std::io::Write;

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
                            
                            // 1. Construct a file path to check
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

                                // B. Do the "Download" (REAL S3 Download)
                                info!(event = "download_start", path = %file_path, "Starting real S3 download...");
                                
                                // Call the function we pasted at the bottom
                                if let Err(e) = download_file_from_s3(&file_path).await {
                                    error!(event = "download_error", error = ?e, "Failed to download from S3");
                                    // In a real app, you might want to retry or crash here
                                }

                                // C. Stop Timer & Record
                                let duration = start.elapsed().as_secs_f64();
                                metrics_state.observe_warmup(duration);
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


// NEW: Real S3 Download Function
async fn download_file_from_s3(target_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup AWS Configuration (pointing to MinIO)
    let region_provider = RegionProviderChain::default_provider().or_else(Region::new("us-east-1"));
    let config = aws_config::from_env()
        .region(region_provider)
        .endpoint_url("http://localhost:9000") // Connects to your MinIO Port-Forward
        .load()
        .await;

    // 2. Create the Client

    let client = S3Client::new(&config);

    // 3. The Download Request
    let bucket = "models";
    let key = "gpt-4-weights"; // The file you just uploaded

    info!(event = "s3_start", bucket = %bucket, key = %key, "Starting S3 download stream");

    let mut resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    // 4. Stream the Data to Disk
    let mut file = File::create(target_path)?;
    
    while let Some(bytes) = resp.body.try_next().await? {
        file.write_all(&bytes)?;
    }

    info!(event = "s3_complete", path = %target_path, "Download finished successfully");
    Ok(())
}