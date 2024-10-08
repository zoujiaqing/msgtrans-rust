use crate::callbacks::{
    OnMessageHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::{Packet, PacketHeader};
use crate::context::Context;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;
use bytes::{BytesMut, Buf};
use crate::session::TransportSession;

pub struct TcpTransportSession {
    reader: Arc<Mutex<tokio::io::ReadHalf<TcpStream>>>,
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    id: usize,
    message_handler: Mutex<Option<Arc<Mutex<OnMessageHandler>>>>,
    close_handler: Mutex<Option<Arc<Mutex<OnCloseHandler>>>>,
    error_handler: Mutex<Option<Arc<Mutex<OnSessionErrorHandler>>>>,
    timeout_handler: Mutex<Option<Arc<Mutex<OnSessionTimeoutHandler>>>>,
}

impl TcpTransportSession {
    pub fn new(stream: TcpStream, id: usize) -> Arc<Self> {
        let (reader, writer) = tokio::io::split(stream);
        Arc::new(TcpTransportSession {
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
            id,
            message_handler: Mutex::new(None),
            close_handler: Mutex::new(None),
            error_handler: Mutex::new(None),
            timeout_handler: Mutex::new(None),
        })
    }
}

#[async_trait]
impl TransportSession for TcpTransportSession {
    async fn send(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut writer = self.writer.lock().await;
        let data = packet.to_bytes();
        writer.write_all(&data).await?;
        writer.flush().await?;
        Ok(())
    }
    
    async fn start_receiving(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut reader = self.reader.lock().await;
        let mut buffer = BytesMut::new();

        loop {
            let mut temp_buf = vec![0; 1024];
            let n = reader.read(&mut temp_buf).await?;
            if n == 0 {
                // Stream has been closed
                if let Some(handler) = self.get_close_handler().await {
                    let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                    handler.lock().await(context);
                }
                break;
            }

            buffer.extend_from_slice(&temp_buf[..n]);

            while buffer.len() >= 16 {
                // Ensure we have enough data to parse the PacketHeader
                let header = PacketHeader::from_bytes(&buffer[..16]);

                // Check if the full packet is available
                let total_length = 16 + header.extend_length as usize + header.message_length as usize;
                if buffer.len() < total_length {
                    break; // Not enough data, wait for more
                }

                // Parse the full packet
                let packet = Packet::by_header_from_bytes(header, &buffer[16..total_length]);

                // Handle the packet
                if let Some(handler) = self.get_message_handler().await {
                    let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                    handler.lock().await(context, packet);
                }

                // Remove the parsed packet from the buffer
                buffer.advance(total_length);
            }
        }

        Ok(())
    }

    async fn close(self: Arc<Self>) {
        let mut send_stream = self.writer.lock().await;
        let _ = send_stream.shutdown().await.unwrap();
    }

    fn id(&self) -> usize {
        self.id
    }

    async fn set_message_handler(self: Arc<Self>, handler: Arc<Mutex<OnMessageHandler>>) {
        let mut message_handler = self.message_handler.lock().await;
        *message_handler = Some(handler);
    }

    async fn get_message_handler(&self) -> Option<Arc<Mutex<OnMessageHandler>>> {
        let message_handler = self.message_handler.lock().await;
        message_handler.clone()
    }

    async fn set_close_handler(self: Arc<Self>, handler: Arc<Mutex<OnCloseHandler>>) {
        let mut close_handler = self.close_handler.lock().await;
        *close_handler = Some(handler);
    }

    async fn get_close_handler(&self) -> Option<Arc<Mutex<OnCloseHandler>>> {
        let close_handler = self.close_handler.lock().await;
        close_handler.clone()
    }

    async fn set_error_handler(self: Arc<Self>, handler: Arc<Mutex<OnSessionErrorHandler>>) {
        let mut error_handler = self.error_handler.lock().await;
        *error_handler = Some(handler);
    }

    async fn get_error_handler(&self) -> Option<Arc<Mutex<OnSessionErrorHandler>>> {
        let error_handler = self.error_handler.lock().await;
        error_handler.clone()
    }

    async fn set_timeout_handler(self: Arc<Self>, handler: Arc<Mutex<OnSessionTimeoutHandler>>) {
        let mut timeout_handler = self.timeout_handler.lock().await;
        *timeout_handler = Some(handler);
    }

    async fn get_timeout_handler(&self) -> Option<Arc<Mutex<OnSessionTimeoutHandler>>> {
        let timeout_handler = self.timeout_handler.lock().await;
        timeout_handler.clone()
    }
}