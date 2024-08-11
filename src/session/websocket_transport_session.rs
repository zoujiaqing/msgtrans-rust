use crate::packet::Packet;
use crate::session::TransportSession;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio::net::TcpStream;
use futures::{stream::StreamExt, sink::SinkExt};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use bytes::BytesMut;

pub struct WebSocketTransportSession {
    ws_stream: WebSocketStream<TcpStream>,
    id: usize,
    message_handler: Option<Arc<Mutex<Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>>>>,
}

impl WebSocketTransportSession {
    pub fn new(ws_stream: WebSocketStream<TcpStream>, id: usize) -> Self {
        WebSocketTransportSession {
            ws_stream,
            id,
            message_handler: None,
        }
    }
}

#[async_trait::async_trait]
impl TransportSession for WebSocketTransportSession {
    async fn send_packet(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 将 Packet 转换为二进制数据并发送
        let data = packet.to_bytes().to_vec(); // 确保数据是 Vec<u8> 类型
        self.ws_stream.send(Message::Binary(data)).await?;
        Ok(())
    }

    async fn process_packet(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 示例：处理收到的 Packet，比如打印内容
        println!("Processing packet with ID: {}", packet.message_id);
    
        // 假设你需要根据收到的 Packet 发送一个响应
        let response_packet = Packet::new(42, b"WebSocket Response".to_vec());
        self.send_packet(response_packet).await?;
    
        Ok(())
    }

    async fn receive_packet(&mut self) -> Option<Packet> {
        if let Some(msg) = self.ws_stream.next().await {
            match msg {
                Ok(Message::Binary(bin)) => Some(Packet::from_bytes(&bin)),
                _ => None,
            }
        } else {
            None
        }
    }

    async fn close(&mut self) {
        self.ws_stream.close(None).await.unwrap();
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_message_handler(
        &mut self,
        handler: Arc<Mutex<Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>>>,
    ) {
        self.message_handler = Some(handler);
    }

    fn get_message_handler(&self) -> Option<Arc<Mutex<Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>>>> {
        self.message_handler.clone()
    }
}