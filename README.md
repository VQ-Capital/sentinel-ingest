# 📈 sentinel-market-ingest (Legacy: sentinel-ingest)

**Domain:** L2 Market Data & Tick Ingestion
**Rol:** Sistemin Gözleri (Sayısal)

Bu servis, borsalardan (Binance vb.) gelen saf matematiksel verilerin (Fiyat, Hacim, L2 Emir Defteri) sisteme girdiği ilk kapıdır. Sıfır gecikme (Zero-Latency) hedefiyle çalışır ve bellekte allocation (tahsis) yapmaktan kaçınır.

- **Kaynaklar:** Binance WSS, Hyperliquid L2
- **NATS Çıktısı:** `market.trade.*`, `market.orderbook.*`
- **SLA Hedefi:** < 10ms