// ========== DOSYA: sentinel-ingest/src/main.rs ==========
use anyhow::{Context, Result};
use async_nats::Client;
use futures_util::StreamExt;
use prost::Message; // Protobuf encoding/decoding trait'i
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use tracing::{error, info, warn};
use tracing_subscriber;

// 1. PROTOBUF'DAN ÜRETİLEN RUST KODUNU İÇERİ ALIYORUZ
// build.rs'nin ürettiği kod bu modüle yerleşir.
pub mod sentinel_market {
    include!(concat!(env!("OUT_DIR"), "/sentinel.market.rs"));
}
use sentinel_market::AggTrade as ProtoAggTrade;

// 2. BİNANCE'İN JSON VERİSİNİ PARSE ETMEK İÇİN YAPI (Edge Katmanı)
// Binance'in gönderdiği formata (e, E, s, p, q...) birebir uyumlu olmalı.
#[derive(Debug, Deserialize)]
struct BinanceAggTrade {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String, // Binance fiyatı string yollar, precision kaybı olmasın diye
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    timestamp: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Loglamayı başlat
    tracing_subscriber::fmt::init();
    info!("🚀 Sentinel-Ingestor başlatılıyor...");

    // 3. NATS BROKER'A BAĞLAN
    // sentinel-infra reposunda docker-compose ile ayağa kaldırdığımız NATS
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let nats_client = async_nats::connect(&nats_url)
        .await
        .context("❌ NATS sunucusuna bağlanılamadı! docker-compose çalışıyor mu?")?;
    info!("✅ NATS Omurgasına bağlanıldı: {}", nats_url);

    // 4. BİNANCE WEBSOCKET'E BAĞLAN (BTCUSDT örneği)
    let binance_ws_url = "wss://stream.binance.com:9443/ws/btcusdt@aggTrade";
    let (ws_stream, _) = connect_async(binance_ws_url)
        .await
        .context("❌ Binance WebSocket'e bağlanılamadı!")?;
    info!("✅ Binance L2 AggTrade Akışına bağlanıldı: {}", binance_ws_url);

    let (_, mut read) = ws_stream.split();

    // 5. ANA İŞLEM DÖNGÜSÜ (HOT LOOP)
    info!("🔥 Veri hortumu açıldı. NATS'a stream ediliyor...");
    
    // Gelen her mesajı asenkron olarak dinle
    while let Some(message) = read.next().await {
        match message {
            Ok(WsMessage::Text(text)) => {
                // Adım A: JSON'u Rust Struct'ına çevir
                match serde_json::from_str::<BinanceAggTrade>(&text) {
                    Ok(binance_trade) => {
                        // Adım B: String olan fiyat ve hacmi double'a (f64) çevir
                        let price: f64 = binance_trade.price.parse().unwrap_or(0.0);
                        let quantity: f64 = binance_trade.quantity.parse().unwrap_or(0.0);

                        // Adım C: Protobuf Modelimizi (Sözleşmemizi) oluştur
                        let proto_msg = ProtoAggTrade {
                            symbol: binance_trade.symbol.clone(),
                            price,
                            quantity,
                            timestamp: binance_trade.timestamp,
                            is_buyer_maker: binance_trade.is_buyer_maker,
                        };

                        // Adım D: Protobuf modelini Bayt dizisine (Bytes) çevir
                        let mut buf = Vec::new();
                        proto_msg.encode(&mut buf)?;

                        // Adım E: NATS üzerindeki ilgili kanala fırlat!
                        // Örn: market.trade.binance.BTCUSDT
                        let subject = format!("market.trade.binance.{}", binance_trade.symbol);
                        
                        if let Err(e) = nats_client.publish(subject.clone(), buf.into()).await {
                            error!("NATS Publish Hatası: {:?}", e);
                        } else {
                            // Ekranda devasa bir hızla aktığını göreceksin
                            info!("📡 [NATS: {}] {} adet {}$'dan işlem gördü.", 
                                subject, quantity, price);
                        }
                    }
                    Err(e) => warn!("JSON Parse Hatası: {:?} - Ham Veri: {}", e, text),
                }
            }
            Ok(WsMessage::Ping(_)) => { /* Binance'e Pong atmayı kütüphane otomatik yapar */ }
            Ok(WsMessage::Close(_)) => {
                warn!("Binance bağlantıyı kapattı.");
                break;
            }
            Err(e) => {
                error!("WebSocket Okuma Hatası: {:?}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}