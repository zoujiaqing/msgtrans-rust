use crate::packet::Packet;
use crate::session::TransportSession;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use bytes::BytesMut;

pub struct TcpTransportSession {
    stream: TcpStream,
    id: usize,
    message_handler: Option<Arc<Mutex<Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>>>>,
}

impl TcpTransportSession {
    pub fn new(stream: TcpStream, id: usize) -> Self {
        TcpTransportSession {
            stream,
            id,
            message_handler: None,
        }
    }
}

#[async_trait::async_trait]
impl TransportSession for TcpTransportSession {

    async fn send_packet(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.stream.write_all(&packet.to_bytes()).await?;
        Ok(())
    }

    async fn process_packet(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 处理收到的 Packet，比如打印或执行其他操作
        println!("Processing packet with ID: {}", packet.message_id);
    
        // 假设这是一个请求，并且需要发送一个响应
        let response_packet = Packet::new(42, b"Response test".to_vec());
        self.send_packet(response_packet).await?;
    
        Ok(())
    }
    
    async fn receive_packet(&mut self) -> Option<Packet> {
        let mut buf = BytesMut::with_capacity(1024);
        let n = self.stream.read_buf(&mut buf).await.unwrap();
        if n > 0 {
            Some(Packet::from_bytes(&buf[..n]))
        } else {
            None
        }
    }

    async fn close(&mut self) {
        self.stream.shutdown().await.unwrap();
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