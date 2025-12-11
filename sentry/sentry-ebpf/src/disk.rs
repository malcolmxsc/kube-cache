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
    // /sys/kernel/debug/tracing/events/block/block_rq_complete/format shows:
    // field:unsigned int nr_sector; offset:24; size:4; signed:0;
    
    let nr_sector: u32 = unsafe { ctx.read_at(24).unwrap_or(0) };
    
    let bytes = nr_sector as u64 * 512;
    
    // Construct Event
    let event = ProbeEvent {
        duration_ns: 0,
        disk_bytes: bytes,
    };
    
    // Output to PerfEventArray
    EVENTS.output(&ctx, &event, 0);
    
    core::result::Result::Ok(0)
}
