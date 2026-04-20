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
use sentinel_market::AggTrade as ProtoAggTrade;

#[derive(Debug, Deserialize)]
struct BinanceAggTrade {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    timestamp: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let nats_client = async_nats::connect(&nats_url)
        .await
        .context("CRITICAL: NATS sunucusuna bağlanılamadı. Sistem başlatılamıyor.")?;

    let binance_ws_url = "wss://stream.binance.com:9443/ws/btcusdt@aggTrade";

    // RECONNECT DÖNGÜSÜ (Bağlantı koparsa NATS düşmez, sadece veri durur ve yeniden dener)
    loop {
        info!("🔄 Binance WebSocket'e bağlanılıyor: {}", binance_ws_url);

        match connect_async(binance_ws_url).await {
            Ok((ws_stream, _)) => {
                info!("✅ Bağlantı başarılı. Veri akışı başladı.");
                let (_, mut read) = ws_stream.split();

                while let Some(message) = read.next().await {
                    match message {
                        Ok(WsMessage::Text(text)) => {
                            if let Ok(trade) = serde_json::from_str::<BinanceAggTrade>(&text) {
                                // SAĞLAMLAŞTIRMA: Parse edilemezse paketi sessizce (veya loglayarak) çöpe at
                                let Ok(price) = trade.price.parse::<f64>() else {
                                    continue;
                                };
                                let Ok(quantity) = trade.quantity.parse::<f64>() else {
                                    continue;
                                };

                                let proto_msg = ProtoAggTrade {
                                    symbol: trade.symbol.clone(),
                                    price,
                                    quantity,
                                    timestamp: trade.timestamp,
                                    is_buyer_maker: trade.is_buyer_maker,
                                };

                                let mut buf = Vec::new();
                                // ZERO-TOLERANCE: unwrap() kullanılamaz!
                                if let Err(e) = proto_msg.encode(&mut buf) {
                                    warn!("⚠️ Protobuf Encode Hatası (Paket Atıldı): {}", e);
                                    continue;
                                }

                                let subject = format!("market.trade.binance.{}", trade.symbol);

                                // NATS gönderim hatasını çökertmek yerine sadece logla
                                if let Err(e) = nats_client.publish(subject, buf.into()).await {
                                    warn!("⚠️ NATS Publish Hatası: {}", e);
                                }
                            }
                        }
                        Ok(WsMessage::Close(_)) => break, // Döngüyü kır, dış loop yeniden bağlansın
                        Err(e) => {
                            error!("❌ WebSocket Okuma Hatası: {:?}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => error!(
                "❌ Bağlantı reddedildi, 3 saniye sonra tekrar denenecek: {:?}",
                e
            ),
        }
        warn!("⚠️ WebSocket koptu. Yeniden bağlanılıyor...");
        sleep(Duration::from_secs(3)).await;
    }
}
