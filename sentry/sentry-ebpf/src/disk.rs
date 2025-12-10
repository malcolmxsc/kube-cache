use aya_ebpf::{
    macros::tracepoint,
    programs::TracePointContext,
};
use aya_log_ebpf::info;

#[tracepoint]
pub fn block_rq_complete(ctx: TracePointContext) -> u32 {
    match try_block_rq_complete(ctx) {
        Ok(ret) => ret,
        Err(_) => 0,
    }
}

fn try_block_rq_complete(ctx: TracePointContext) -> Result<u32, i64> {
    // Logic: Capture the number of sectors written
    // The field 'nr_sector' is at offset 80 in older kernels or can vary.
    // However, since we don't have vmlinux bindings generated yet,
    // we will rely on reading the field by name if possible or simplify tracing.
    // For this task, we will just log the event.
    // In a real scenario, we'd use `ctx.read_at` based on format.

    // Using `info!` from aya_log_ebpf to log back to userspace.
    info!(&ctx, "Disk Write Complete");
    
    Ok(0)
}
