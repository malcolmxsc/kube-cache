use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::metrics::SdkMeterProvider; // v0.23 compatible
use prometheus::Registry;

#[derive(Clone)]
pub struct MetricsState {
    pub registry: Registry,
}

impl MetricsState {
    pub fn new() -> Self {
        // 1. Create a Registry
        let registry = Registry::new();

        // 2. Configure the Exporter
        // opentelemetry-prometheus 0.16 returns a type compatible with SDK 0.23
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .build()
            .unwrap();

        // 3. Create the Provider
        let provider = SdkMeterProvider::builder()
            .with_reader(exporter)
            .build();

        // 4. Register globally
        global::set_meter_provider(provider);
        
        MetricsState { registry }
    }

    pub fn record_prewarm_success(&self, dataset: &str) {
        let meter = global::meter("kube_cache");
        
        let counter = meter
            .u64_counter("gpu_prewarm_success_total")
            .with_description("Total number of GPU pods pre-warmed")
            .init();

        counter.add(1, &[KeyValue::new("dataset", dataset.to_string())]);
    }
}
