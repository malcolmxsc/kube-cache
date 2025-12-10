// --- IMPORTS ---
use kube::{Api, Client, api::{WatchEvent, WatchParams, Patch, PatchParams}};
use k8s_openapi::api::core::v1::Pod;
use futures::StreamExt;
use serde_json::json;
use rustls::crypto::ring;

// IMPORTS FOR SPANS AND TRACES
use opentelemetry::{KeyValue};
use opentelemetry_sdk::{trace as sdktrace, Resource};
// FIXED: Added this back so .with_endpoint() works
use opentelemetry_otlp::WithExportConfig; 
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};

// --- NEW IMPORTS FOR S3 ---
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{Client as S3Client, config::Region};
use std::fs::File;
use std::io::Write;

// NEW: Logging Imports
use tracing::{info, error}; // Removed unused 'Level'

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
    info!(event = "server_start", port = 8080, "Metrics Server listening");
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn init_telemetry() {
    // 1. Create the OTLP (OpenTelemetry) Exporter
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint("http://tempo:4317"); 

    // 2. Define the Tracer
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            sdktrace::config().with_resource(Resource::new(vec![
                KeyValue::new("service.name", "kube-cache"), 
            ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .expect("Failed to install OTLP tracer");

    // 3. Connect Tracing to Logs (Stdout) AND Traces (Tempo)
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let logger = tracing_subscriber::fmt::layer().json(); 
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());

    // 4. Register everything
    Registry::default()
        .with(env_filter)
        .with(logger)
        .with(telemetry)
        .init();
}

// --- MAIN OPERATOR LOOP ---
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize Telemetry (Logs + Traces)
    init_telemetry();

    // 2. Install Crypto Provider
    ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // 3. Initialize the Observability Layer
    let metrics_state = MetricsState::new();
    
    // 4. Spawn the Web Server
    let server_state = metrics_state.clone();
    tokio::spawn(async move {
        start_metrics_server(server_state).await;
    });

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");
    
    let gate_name = "kube-cache.openai.com/gate";
    let wp = WatchParams::default();

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
                    info!(event = "pod_locked", pod_name = %name, "Locked Pod Detected");
                    
                    if let Some(annotations) = pod.metadata.annotations {
                        if let Some(data_url) = annotations.get("x-openai/required-dataset") {
                            
                            info!(event = "delegation_start", pod_name = %name, dataset = %data_url, "Delegating download to job");
                            
                            let filename = data_url.replace("s3://", "").replace("/", "-");
                            let file_path = format!("/tmp/{}", filename);

                            if std::path::Path::new(&file_path).exists() {
                                info!(event = "cache_hit", pod_name = %name, path = %file_path, "Dataset found locally");
                                metrics_state.count_hit();
                            } else {
                                info!(event = "cache_miss", pod_name = %name, path = %file_path, "Downloading dataset");
                                metrics_state.count_miss();

                                let start = std::time::Instant::now();

                                info!(event = "download_start", path = %file_path, "Starting real S3 download...");
                                
                                if let Err(e) = download_file_from_s3(&file_path).await {
                                    error!(event = "download_error", error = ?e, "Failed to download from S3");
                                }

                                let duration = start.elapsed().as_secs_f64();
                                metrics_state.observe_warmup(duration);
                            }

                            info!(event = "data_ready", pod_name = %name, "Data ready on disk");

                            let patch = json!({
                                "spec": { "schedulingGates": [] }
                            });
                            
                            let pp = PatchParams::default();
                            pods.patch(&name, &pp, &Patch::Merge(patch)).await?;
                            
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
#[tracing::instrument(fields(bucket="models", key="gpt-4-weights"))]
async fn download_file_from_s3(target_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let region_provider = RegionProviderChain::default_provider().or_else(Region::new("us-east-1"));

    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:9000".to_string());

    info!(event = "config_check", endpoint = %s3_endpoint, "Connecting to S3 Storage");

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region_provider)
        .endpoint_url(&s3_endpoint)
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .force_path_style(true)
        .build();

    let client = S3Client::from_conf(s3_config);

    let bucket = "models";
    let key = "gpt-4-weights";

    info!(event = "s3_start", bucket = %bucket, key = %key, "Starting S3 download stream");

    let mut resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let mut file = File::create(target_path)?;
    
    while let Some(bytes) = resp.body.try_next().await? {
        file.write_all(&bytes)?;
    }

    info!(event = "s3_complete", path = %target_path, "Download finished successfully");
    Ok(())
}