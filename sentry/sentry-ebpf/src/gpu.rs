#![no_std]
#![no_main]

use aya_ebpf::{
    macros::uprobe,
    programs::ProbeContext,
};
use aya_log_ebpf::info;

#[uprobe]
pub fn cuda_launch_kernel(ctx: ProbeContext) -> u32 {
    match try_cuda_launch_kernel(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_cuda_launch_kernel(ctx: ProbeContext) -> Result<u32, u32> {
    info!(&ctx, "GPU Kernel Launched");
    Ok(0)
}
