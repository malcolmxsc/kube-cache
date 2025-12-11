#![no_std]

use aya_ebpf::{
    macros::map,
    maps::PerfEventArray,
};
use sentry_common::ProbeEvent;

pub mod disk;
pub mod net;
pub mod gpu;

#[map]
pub static EVENTS: PerfEventArray<ProbeEvent> = PerfEventArray::new(0);
