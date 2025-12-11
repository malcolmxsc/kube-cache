use aya_ebpf::{
    macros::{kprobe, tracepoint, map},
    programs::{ProbeContext, TracePointContext},
    maps::HashMap,
    helpers::{bpf_ktime_get_ns, bpf_get_current_pid_tgid},
};
use sentry_common::ProbeEvent;
use crate::EVENTS;

// Map to store start time of connection Key: PID/TGID (u64), Value: Timestamp (u64)
#[map]
pub static SOCKET_START: HashMap<u64, u64> = HashMap::with_max_entries(1024, 0);

// Capture Start Time
#[kprobe]
pub fn tcp_connect(ctx: ProbeContext) -> u32 {
    match try_tcp_connect(ctx) {
        core::result::Result::Ok(ret) => ret,
        core::result::Result::Err(_) => 0,
    }
}

fn try_tcp_connect(_ctx: ProbeContext) -> core::result::Result<u32, i64> {
    let pid = bpf_get_current_pid_tgid();
    let start_time = unsafe { bpf_ktime_get_ns() };
    
    // Store in map
    SOCKET_START.insert(&pid, &start_time, 0)?;
    
    core::result::Result::Ok(0)
}

// Calculate Duration using TracePoint (Robust alternative to KRetProbe)
#[tracepoint]
pub fn tcp_connect_end(ctx: TracePointContext) -> u32 {
    let pid = bpf_get_current_pid_tgid();
    
    // Lookup start time
    if let core::option::Option::Some(start_time) = unsafe { SOCKET_START.get(&pid) } {
        let end_time = unsafe { bpf_ktime_get_ns() };
        let duration_ns = end_time - *start_time;
        
        // Construct Event
        let event = ProbeEvent {
            duration_ns,
            disk_bytes: 0,
        };
        
        // Output to PerfEventArray. ignore error.
        EVENTS.output(&ctx, &event, 0);
        
        // Clean up map
        let _ = SOCKET_START.remove(&pid);
    }
    
    0
}
