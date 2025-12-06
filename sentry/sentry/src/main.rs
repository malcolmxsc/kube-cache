use aya::programs::{Xdp, XdpFlags};
use aya::{include_bytes_aligned, Ebpf};
use aya::maps::HashMap;
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

    let program: &mut Xdp = bpf.program_mut("sentry").unwrap().try_into()?;
    program.load()?;
    program.attach("eth0", XdpFlags::SKB_MODE)?;

    println!("🛡️  Sentry Attached to eth0! Monitoring traffic...");
    println!("--- DEBUG MODE: v4 (NON-BLOCKING FIX) ---"); 

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
