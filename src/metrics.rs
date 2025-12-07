use prometheus::{
    IntCounter, Histogram, HistogramOpts, Registry, 
    IntGauge, opts, register_int_counter_with_registry, 
    register_histogram_with_registry, register_int_gauge_with_registry
};

use std::sync::Arc;
// use std::sync::OnceLock; // You can remove this if unused

#[derive(Clone)]
pub struct MetricsState {
    // FIX 1: We use Arc here so cloning the Struct doesn't clone the Registry completely
    pub registry: Arc<Registry>,
    
    // 1. The Scoreboard (Counters)
    pub ops_prewarm_success: IntCounter,
    pub ops_cache_hit: IntCounter,
    pub ops_cache_miss: IntCounter,

    // 2. The Stopwatch (Histograms)
    pub latency_warmup: Histogram,
    pub latency_queue: Histogram,

    // 3. The Speedometer (Gauges)
    pub throughput_nvme: IntGauge,
    pub gpu_idle_seconds: IntGauge,
}

impl MetricsState {
    pub fn new() -> Self {
        let registry = Registry::new();

        // --- 1. Counters ---
        let ops_prewarm_success = register_int_counter_with_registry!(
            opts!("gpu_prewarm_success_total", "Total successful GPU pre-warm operations"),
            registry
        ).unwrap();

        let ops_cache_hit = register_int_counter_with_registry!(
            opts!("cache_hit_total", "Total times data was found locally"),
            registry
        ).unwrap();

        let ops_cache_miss = register_int_counter_with_registry!(
            opts!("cache_miss_total", "Total times data had to be downloaded"),
            registry
        ).unwrap();

        // --- 2. Histograms ---
        let bucket_opts = HistogramOpts::new("warmup_latency_seconds", "Time taken to download data")
            .buckets(vec![1.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0]);
            
        let latency_warmup = register_histogram_with_registry!(
            bucket_opts,
            registry
        ).unwrap();

        let queue_opts = HistogramOpts::new("gpu_job_queue_time_seconds", "Time a Pod waits in the gate");
        let latency_queue = register_histogram_with_registry!(
            queue_opts,
            registry
        ).unwrap();

        // --- 3. Gauges ---
        let throughput_nvme = register_int_gauge_with_registry!(
            opts!("nvme_read_throughput_bytes", "Current read speed of NVMe cache"),
            registry
        ).unwrap();

        let gpu_idle_seconds = register_int_gauge_with_registry!(
            opts!("gpu_idle_seconds", "Seconds the GPU sat doing nothing"),
            registry
        ).unwrap();

        Self {
            // FIX 2: We wrap the registry in Arc::new() so it can be shared!
            registry: Arc::new(registry), 
            ops_prewarm_success,
            ops_cache_hit,
            ops_cache_miss,
            latency_warmup,
            latency_queue,
            throughput_nvme,
            gpu_idle_seconds,
        }
    }

    // --- Helper Functions to Record Data ---

    pub fn count_success(&self) {
        self.ops_prewarm_success.inc();
    }

    pub fn count_hit(&self) {
        self.ops_cache_hit.inc();
    }

    pub fn count_miss(&self) {
        self.ops_cache_miss.inc();
    }

    pub fn observe_warmup(&self, seconds: f64) {
        self.latency_warmup.observe(seconds);
    }
}