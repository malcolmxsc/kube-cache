use aya::programs::{Xdp, XdpFlags, KProbe, TracePoint};
use aya::{include_bytes_aligned, Ebpf};
use aya::maps::HashMap;
use aya_log::EbpfLogger;
use std::convert::TryInto;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    #[cfg(debug_assertions)]
    let mut bpf = Ebpf::load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/debug/sentry"
    ))?;
    #[cfg(not(debug_assertions))]
    let mut bpf = Ebpf::load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/release/sentry"
    ))?;

    // 0. Init Logger (CRITICAL for info! macro)
    if let Err(e) = EbpfLogger::init(&mut bpf) {
        // This can fail if there are no logs; we just warn.
        eprintln!("failed to initialize eBPF logger: {}", e);
    }

    // 1. XDP (Existing)
    let program: &mut Xdp = bpf.program_mut("sentry").unwrap().try_into()?;
    program.load()?;
    program.attach("eth0", XdpFlags::SKB_MODE)?;
    println!("🛡️  Sentry XDP Attached to eth0");

    // 2. KProbe (Network Latency)
    let kprobe: &mut KProbe = bpf.program_mut("tcp_connect").unwrap().try_into()?;
    kprobe.load()?;
    kprobe.attach("tcp_connect", 0)?;
    println!("🔌 Sentry KProbe Attached to tcp_connect");

    // 3. TracePoint (Disk Latency)
    let tp: &mut TracePoint = bpf.program_mut("block_rq_complete").unwrap().try_into()?;
    tp.load()?;
    tp.attach("block", "block_rq_complete")?;
    println!("💾 Sentry TracePoint Attached to block/block_rq_complete");

    println!("--- DEBUG MODE: v5 (DEEP OBSERVABILITY) ---"); 

    let packet_counts: HashMap<_, u32, u32> = HashMap::try_from(bpf.map_mut("PACKET_COUNTS").unwrap())?;

    loop {
        // REMOVED THE BLOCKING signal::ctrl_c().await HERE

        let mut total_packets = 0;
        for item in packet_counts.iter() {
            if let Ok((_port, count)) = item {
                total_packets += count;
            }
        }

        println!("📦 Total Packets Intercepted: {}", total_packets);

        thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}
