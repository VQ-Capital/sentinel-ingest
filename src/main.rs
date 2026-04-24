// ========== DOSYA: sentinel-ingest/src/main.rs ==========
use anyhow::{Context, Result};
use futures_util::StreamExt;
use prost::Message;
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use tracing::{error, info, warn};

pub mod sentinel_market {
    include!(concat!(env!("OUT_DIR"), "/sentinel.market.v1.rs"));
}
use sentinel_market::{AggTrade as ProtoAggTrade, OrderbookDepth, PriceLevel};

#[derive(Debug, Deserialize)]
struct BinanceAggTrade {
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    timestamp: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

// Orderbook Depth için Binance Veri Formatı
#[derive(Debug, Deserialize)]
struct BinanceDepth {
    bids: Vec<[String; 2]>, // [Fiyat, Miktar]
    asks: Vec<[String; 2]>, // [Fiyat, Miktar]
}

// Gelen JSON Polymorphic (Çok Biçimli) olduğu için Untagged Enum kullanıyoruz (Zero-Allocation Parsing)
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BinanceData {
    Trade(BinanceAggTrade),
    Depth(BinanceDepth),
}

#[derive(Debug, Deserialize)]
struct BinanceStreamEvent {
    stream: String,
    data: BinanceData,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let nats_client = async_nats::connect(&nats_url)
        .await
        .context("CRITICAL: NATS sunucusuna bağlanılamadı.")?;

    let symbols = ["btcusdt", "ethusdt", "solusdt", "bnbusdt"];
    let mut streams = Vec::new();
    for s in symbols.iter() {
        streams.push(format!("{}@aggTrade", s));
        streams.push(format!("{}@depth10@100ms", s)); // YENİ: L2 ORDERBOOK DERİNLİĞİ EKLENDİ
    }

    let binance_ws_url = format!(
        "wss://stream.binance.com:9443/stream?streams={}",
        streams.join("/")
    );

    loop {
        info!(
            "🔄 Binance Multi-Stream (Trade + Depth) Bağlanılıyor: {}",
            binance_ws_url
        );

        match connect_async(&binance_ws_url).await {
            Ok((ws_stream, _)) => {
                info!(
                    "✅ [HFT-DATA] Multi-Stream Bağlantı başarılı. L2 Orderbook ve Trades Akıyor."
                );
                let (_, mut read) = ws_stream.split();

                while let Some(message) = read.next().await {
                    if let Ok(WsMessage::Text(text)) = message {
                        if let Ok(event) = serde_json::from_str::<BinanceStreamEvent>(&text) {
                            // Stream adından sembolü çıkar (örn: "btcusdt@aggTrade" -> "BTCUSDT")
                            let symbol = event
                                .stream
                                .split('@')
                                .next()
                                .unwrap_or("UNKNOWN")
                                .to_uppercase();

                            match event.data {
                                BinanceData::Trade(trade) => {
                                    if let (Ok(price), Ok(quantity)) =
                                        (trade.price.parse::<f64>(), trade.quantity.parse::<f64>())
                                    {
                                        let proto_msg = ProtoAggTrade {
                                            symbol: symbol.clone(),
                                            price,
                                            quantity,
                                            timestamp: trade.timestamp,
                                            is_buyer_maker: trade.is_buyer_maker,
                                        };
                                        let mut buf = Vec::new();
                                        if proto_msg.encode(&mut buf).is_ok() {
                                            let _ = nats_client
                                                .publish(
                                                    format!("market.trade.binance.{}", symbol),
                                                    buf.into(),
                                                )
                                                .await;
                                        }
                                    }
                                }
                                BinanceData::Depth(depth) => {
                                    // Orderbook Protobuf Dönüşümü
                                    let mut proto_bids = Vec::with_capacity(depth.bids.len());
                                    for b in depth.bids {
                                        if let (Ok(p), Ok(q)) =
                                            (b[0].parse::<f64>(), b[1].parse::<f64>())
                                        {
                                            proto_bids.push(PriceLevel {
                                                price: p,
                                                quantity: q,
                                            });
                                        }
                                    }

                                    let mut proto_asks = Vec::with_capacity(depth.asks.len());
                                    for a in depth.asks {
                                        if let (Ok(p), Ok(q)) =
                                            (a[0].parse::<f64>(), a[1].parse::<f64>())
                                        {
                                            proto_asks.push(PriceLevel {
                                                price: p,
                                                quantity: q,
                                            });
                                        }
                                    }

                                    let proto_msg = OrderbookDepth {
                                        symbol: symbol.clone(),
                                        bids: proto_bids,
                                        asks: proto_asks,
                                        timestamp: chrono::Utc::now().timestamp_millis(),
                                    };

                                    let mut buf = Vec::new();
                                    if proto_msg.encode(&mut buf).is_ok() {
                                        let _ = nats_client
                                            .publish(
                                                format!("market.orderbook.binance.{}", symbol),
                                                buf.into(),
                                            )
                                            .await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => error!("❌ Bağlantı reddedildi: {:?}", e),
        }
        warn!("⚠️ WebSocket koptu. 3 saniye sonra yeniden bağlanılıyor...");
        sleep(Duration::from_secs(3)).await;
    }
}
