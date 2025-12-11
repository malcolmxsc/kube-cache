#![no_std]
#![no_main]

use aya_ebpf::{
    macros::{xdp, map},
    programs::XdpContext,
    bindings::xdp_action,
    maps::HashMap,
};
use sentry_ebpf::disk::block_rq_complete;
use sentry_ebpf::net::{tcp_connect, tcp_connect_end};
use sentry_ebpf::gpu::gpu_probe_placeholder;

// 1. Define the Map (The Scoreboard)
#[map]
pub static PACKET_COUNTS: HashMap<u32, u32> = HashMap::with_max_entries(1024, 0);

#[xdp]
pub fn sentry(ctx: XdpContext) -> u32 {
    match try_sentry(ctx) {
        Ok(ret) => ret,
        Err(_) => xdp_action::XDP_ABORTED,
    }
}

fn try_sentry(_ctx: XdpContext) -> Result<u32, ()> {
    // 2. INCREMENT THE MAP
    // This logic ensures the map is used, so the compiler keeps it.
    // It also drives the numbers in your dashboard.
    
    let key = 0u32;
    
    // Read current count
    let new_count = match unsafe { PACKET_COUNTS.get(&key) } {
        Some(count) => count + 1,
        None => 1,
    };

    // Write new count
    PACKET_COUNTS.insert(&key, &new_count, 0).map_err(|_| ())?;

    Ok(xdp_action::XDP_PASS)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

// Dummy function to ensure imports are used and code is linked
fn _linker_fix() {
    let _ = block_rq_complete(unsafe { core::mem::zeroed() });
    let _ = tcp_connect(unsafe { core::mem::zeroed() });
    let _ = sentry_ebpf::net::tcp_connect_end(unsafe { core::mem::zeroed() });
    gpu_probe_placeholder();
}