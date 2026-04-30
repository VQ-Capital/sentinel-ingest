// ========== DOSYA: sentinel-ingest/src/config.rs ==========
#[derive(Clone, Debug)]
pub struct IngestConfig {
    pub nats_url: String,
    pub active_symbols: Vec<String>,
}

impl IngestConfig {
    pub fn from_env() -> Self {
        let symbols_env = std::env::var("ACTIVE_SYMBOLS").unwrap_or_else(|_| "BTCUSDT".to_string());
        Self {
            nats_url: std::env::var("NATS_URL")
                .unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            active_symbols: symbols_env
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
        }
    }
}
