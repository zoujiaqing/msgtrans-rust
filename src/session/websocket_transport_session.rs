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
    async fn process_packet(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = packet.to_bytes().to_vec();
        self.ws_stream.send(Message::Binary(data)).await?;
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