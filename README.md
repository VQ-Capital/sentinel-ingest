# 📡 sentinel-market-ingest

Görev: Borsalardan gelen anlık fiyat ve tahta verisi.

Kaynaklar: Binance, Hyperliquid, OKX.

NATS Subject: market.trade.*, market.orderbook.*

Karakter: Mikrosaniye hassasiyetli, en hızlı döngü.

---

Karakter: Sıfır gecikme (Zero-Latency). Sadece borsa WebSockets/FIX.

Veri: Trade, L2 Orderbook, Liquidations.

SaaS Değeri: Kurumsal yatırımcıya "En Temiz Ham Fiyat Akışı" olarak satılır.