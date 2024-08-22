use crate::callbacks::{
    OnMessageHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::Packet;
use crate::context::Context;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;
use crate::session::TransportSession;

pub struct TcpTransportSession {
    reader: Arc<Mutex<tokio::io::ReadHalf<TcpStream>>>, // 读流
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>, // 写流
    id: usize,
    message_handler: Mutex<Option<OnMessageHandler>>,
    close_handler: Mutex<Option<OnCloseHandler>>,
    error_handler: Mutex<Option<OnSessionErrorHandler>>,
    timeout_handler: Mutex<Option<OnSessionTimeoutHandler>>,
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
    async fn send_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut writer = self.writer.lock().await;
        let data = packet.to_bytes();
        writer.write_all(&data).await?;
        writer.flush().await?;
        Ok(())
    }
    
    async fn start_receiving(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut reader = self.reader.lock().await;
        let mut buf = vec![0; 1024];

        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                // 流已关闭
                if let Some(handler) = self.get_close_handler().await {
                    let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                    handler.lock().await(context);
                }
                break;
            }

            let packet = Packet::from_bytes(&buf[..n]);
            if let Some(handler) = self.get_message_handler().await {
                let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                handler.lock().await(context, packet);
            }
        }

        Ok(())
    }

    async fn close_session(self: Arc<Self>, context: Arc<Context>) {
        if let Some(handler) = self.get_close_handler().await {
            handler.lock().await(context);
        }
    }

    fn id(&self) -> usize {
        self.id
    }

    async fn set_message_handler(self: Arc<Self>, handler: OnMessageHandler) {
        let mut message_handler = self.message_handler.lock().await;
        *message_handler = Some(handler);
    }

    async fn get_message_handler(&self) -> Option<OnMessageHandler> {
        let message_handler = self.message_handler.lock().await;
        message_handler.clone()
    }

    async fn set_close_handler(self: Arc<Self>, handler: OnCloseHandler) {
        let mut close_handler = self.close_handler.lock().await;
        *close_handler = Some(handler);
    }

    async fn get_close_handler(&self) -> Option<OnCloseHandler> {
        let close_handler = self.close_handler.lock().await;
        close_handler.clone()
    }

    async fn set_error_handler(self: Arc<Self>, handler: OnSessionErrorHandler) {
        let mut error_handler = self.error_handler.lock().await;
        *error_handler = Some(handler);
    }

    async fn get_error_handler(&self) -> Option<OnSessionErrorHandler> {
        let error_handler = self.error_handler.lock().await;
        error_handler.clone()
    }

    async fn set_timeout_handler(self: Arc<Self>, handler: OnSessionTimeoutHandler) {
        let mut timeout_handler = self.timeout_handler.lock().await;
        *timeout_handler = Some(handler);
    }

    async fn get_timeout_handler(&self) -> Option<OnSessionTimeoutHandler> {
        let timeout_handler = self.timeout_handler.lock().await;
        timeout_handler.clone()
    }
}