#![no_std]

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProbeEvent {
    pub duration_ns: u64,
    pub disk_bytes: u64,
}
