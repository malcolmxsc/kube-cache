use aya_ebpf::{
    macros::tracepoint,
    programs::TracePointContext,
};
use sentry_common::ProbeEvent;
use crate::EVENTS;

#[tracepoint]
pub fn block_rq_complete(ctx: TracePointContext) -> u32 {
    match try_block_rq_complete(ctx) {
        core::result::Result::Ok(ret) => ret,
        core::result::Result::Err(_) => 0,
    }
}

fn try_block_rq_complete(ctx: TracePointContext) -> core::result::Result<u32, i64> {
    // Logic: Capture the number of sectors written
    // Safety: probing arbitrary memory.
    let nr_sector: u64 = unsafe { ctx.read_at(80).unwrap_or(0) }; 
    
    let bytes = nr_sector * 512;
    
    // Construct Event
    let event = ProbeEvent {
        duration_ns: 0,
        disk_bytes: bytes,
    };
    
    // Output to PerfEventArray
    EVENTS.output(&ctx, &event, 0);
    
    core::result::Result::Ok(0)
}
