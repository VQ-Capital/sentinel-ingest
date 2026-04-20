# 📡 sentinel-ingest (The Vacuum)

**Sorumluluk:** Dış dünyadan (Binance, Hyperliquid) gelen anlık WebSocket verilerini (Trade, Orderbook) mikrosaniye gecikmeyle yakalamak.
**Kural:** Bu servis hiçbir veritabanına bağlanmaz. Hiçbir hesaplama yapmaz.
**Çıktı:** Gelen JSON verisini `sentinel-spec/proto/market_data.proto` yapısına dönüştürür ve `NATS JetStream` üzerindeki `market.trade.*` kanallarına fırlatır.
**Dil:** Rust (`tokio`, `tokio-tungstenite`, `async-nats`)