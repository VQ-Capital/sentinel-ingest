// ========== DOSYA: sentinel-ingest/src/main.rs ==========
use anyhow::{Context, Result};
use futures_util::StreamExt;
use prost::Message;
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use tracing::{error, info, warn};

mod config;
use config::IngestConfig;

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

#[derive(Debug, Deserialize)]
struct BinanceDepth {
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

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
    let config = IngestConfig::from_env();

    info!(
        "📡 Service: {} | Version: 0.2.1 (V7 Dynamic Symbols)",
        env!("CARGO_PKG_NAME")
    );

    let nats_client = async_nats::connect(&config.nats_url)
        .await
        .context("NATS Error")?;

    let mut streams = Vec::new();
    for s in config.active_symbols.iter() {
        let lower_s = s.to_lowercase();
        streams.push(format!("{}@aggTrade", lower_s));
        streams.push(format!("{}@depth10@100ms", lower_s));
    }

    let binance_ws_url = format!(
        "wss://stream.binance.com:9443/stream?streams={}",
        streams.join("/")
    );

    loop {
        info!(
            "🔄 Binance Multi-Stream | Symbols: {:?}",
            config.active_symbols
        );

        match connect_async(&binance_ws_url).await {
            Ok((ws_stream, _)) => {
                info!("✅ [HFT-DATA] Multi-Stream Bağlantı başarılı.");
                let (_, mut read) = ws_stream.split();

                while let Some(message) = read.next().await {
                    if let Ok(WsMessage::Text(text)) = message {
                        if let Ok(event) = serde_json::from_str::<BinanceStreamEvent>(&text) {
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
