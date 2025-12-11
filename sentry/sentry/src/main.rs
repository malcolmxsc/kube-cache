use aya::programs::{Xdp, XdpFlags, KProbe, TracePoint};
use aya::{include_bytes_aligned, Ebpf};
use aya_log::EbpfLogger;
use clap::Parser;
use log::{debug, warn, info};
use tokio::signal;
use aya::maps::AsyncPerfEventArray;
use aya::util::online_cpus;
use bytes::BytesMut;
use sentry_common::ProbeEvent;
use prometheus::{Encoder, TextEncoder, register_histogram, register_counter, histogram_opts, opts};
use tiny_http::{Server, Response, Header};
use std::thread;

#[derive(Debug, Parser)]
struct Opt {
    
}

fn register_metrics() -> (prometheus::Histogram, prometheus::Counter) {
    let latency = register_histogram!(histogram_opts!(
        "sentry_tcp_connect_latency_seconds",
        "TCP connection latency in seconds",
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )).unwrap();
    
    let bytes = register_counter!(opts!(
        "sentry_disk_bytes_total",
        "Total bytes written to disk"
    )).unwrap();
    
    (latency, bytes)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    // Bump the memlock rlimit
    let rlim = libc::rlimit {
        rlim_cur: libc::RLIM_INFINITY,
        rlim_max: libc::RLIM_INFINITY,
    };
    let ret = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlim) };
    if ret != 0 {
        debug!("remove limit on locked memory failed, ret is: {}", ret);
    }

    // 0. Initialize Metrics
    let (histogram, counter) = register_metrics();
    
    // 0.5 Start HTTP Server
    thread::spawn(|| {
        match Server::http("0.0.0.0:9091") {
            Ok(server) => {
                println!("🚀 Metrics Server running on :9091");
                for request in server.incoming_requests() {
                    if request.url() == "/metrics" {
                        let mut buffer = vec![];
                        let encoder = TextEncoder::new();
                        let metric_families = prometheus::gather();
                        encoder.encode(&metric_families, &mut buffer).unwrap();
                        
                        let header = Header::from_bytes(&b"Content-Type"[..], &b"text/plain; version=0.0.4"[..]).unwrap();
                        let response = Response::from_data(buffer).with_header(header);
                        let _ = request.respond(response);
                    } else {
                         let response = Response::from_string("Try /metrics");
                         let _ = request.respond(response);
                    }
                }
            },
            Err(e) => {
                eprintln!("Failed to start metrics server: {}", e);
            }
        }
    });

    // 1. Load the eBPF Program (Normal loading, no leak needed)
    let mut bpf = Ebpf::load(include_bytes_aligned!("../../target/bpfel-unknown-none/release/sentry"))?;
    
    // Initialize eBPF logger
    if let Err(e) = EbpfLogger::init(&mut bpf) {
        warn!("failed to initialize eBPF logger: {}", e);
    }
    
    // 2. Load and Attach Probes
    {
        let program: &mut Xdp = bpf.program_mut("sentry").unwrap().try_into()?;
        program.load()?;
        program.attach("eth0", XdpFlags::SKB_MODE)?;
        println!("🛡️  Sentry XDP Attached to eth0");
    }

    {
        let kprobe: &mut KProbe = bpf.program_mut("tcp_connect").unwrap().try_into()?;
        kprobe.load()?;
        kprobe.attach("tcp_connect", 0)?;
        println!("🔌 Sentry KProbe Attached to tcp_connect (Start Timer)");
    }

    {
        let tp_net: &mut TracePoint = bpf.program_mut("tcp_connect_end").unwrap().try_into()?;
        tp_net.load()?;
        tp_net.attach("sock", "inet_sock_set_state")?;
        println!("⏱️  Sentry TracePoint Attached to sock/inet_sock_set_state (End Timer)");
    }

    {
        let tp: &mut TracePoint = bpf.program_mut("block_rq_complete").unwrap().try_into()?;
        tp.load()?;
        tp.attach("block", "block_rq_complete")?;
        println!("💾 Sentry TracePoint Attached to block/block_rq_complete");
    }
    
    // 1.5 Get a handle to the PerfEventArray (Create AFTER probes to avoid borrow conflicts)
    let mut events: AsyncPerfEventArray<_> = bpf.map_mut("EVENTS").unwrap().try_into()?;
    
    // 3. Event Loop (Single CPU, Local Async Loop)
    println!("🎧 Listening for eBPF events on CPU 0...");
    let cpu_id = 0;
    let mut buf = events.open(cpu_id, None)?;
    
    let mut buffers = (0..10)
        .map(|_| BytesMut::with_capacity(1024))
        .collect::<Vec<_>>();

    loop {
        tokio::select! {
            res = buf.read_events(&mut buffers) => {
                let events = res.unwrap();
                for i in 0..events.read {
                    let buf = &mut buffers[i];
                    let ptr = buf.as_ptr() as *const ProbeEvent;
                    let event = unsafe { *ptr };
                    
                    if event.duration_ns > 0 {
                         let duration_secs = event.duration_ns as f64 / 1_000_000_000.0;
                         histogram.observe(duration_secs);
                         println!("[METRIC] TCP Latency: {:.4}s", duration_secs);
                    }
                    
                    if event.disk_bytes > 0 {
                         counter.inc_by(event.disk_bytes as f64);
                         println!("[METRIC] Disk Write: {} bytes", event.disk_bytes);
                    } else if event.disk_bytes == 0 && event.duration_ns == 0 {
                         println!("[METRIC] Disk Write: 0 bytes (Stub)");
                    }
                }
            }
            _ = signal::ctrl_c() => {
                info!("Ctrl-C received, exiting...");
                break;
            }
        }
    }

    Ok(())
}
