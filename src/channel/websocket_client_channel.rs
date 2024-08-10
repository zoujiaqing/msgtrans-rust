use super::ClientChannel;
use crate::packet::Packet;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::MaybeTlsStream;
use futures::stream::StreamExt;
use url::Url;
use futures::sink::SinkExt;

pub struct WebSocketClientChannel {
    ws_stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    address: String,
    port: u16,
}

impl WebSocketClientChannel {
    pub fn new(address: &str, port: u16) -> Self {
        WebSocketClientChannel {
            ws_stream: None,
            address: address.to_string(),
            port,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for WebSocketClientChannel {
    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = Url::parse(&format!("ws://{}:{}/ws", self.address, self.port))?;
        let (ws_stream, _) = connect_async(url).await?;
        self.ws_stream = Some(ws_stream);
        Ok(())
    }

    async fn send(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ws_stream) = &mut self.ws_stream {
            let data = packet.to_bytes().to_vec();  // 转换为 Vec<u8>
            ws_stream.send(Message::Binary(data)).await?;  // 使用 send 而不是 send_all
        }
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ws_stream) = &mut self.ws_stream {
            if let Some(msg) = ws_stream.next().await {
                match msg? {
                    Message::Binary(bin) => {
                        let packet = Packet::from_bytes(&bin);
                        return Ok(Some(packet));
                    }
                    _ => return Ok(None),
                }
            }
        }
        Ok(None)
    }
}