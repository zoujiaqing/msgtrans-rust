use crate::callbacks::{
    OnMessageHandler, OnReceiveHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::Packet;
use crate::context::Context;
use crate::session::TransportSession;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;

pub struct TcpTransportSession {
    stream: Mutex<TcpStream>, // 不再需要 Arc，因为整个结构体已经被 Arc 包装
    id: usize,
    message_handler: Mutex<Option<OnMessageHandler>>, // 使用 Mutex 保护可变字段
    receive_handler: Mutex<Option<OnReceiveHandler>>,
    close_handler: Mutex<Option<OnCloseHandler>>,
    error_handler: Mutex<Option<OnSessionErrorHandler>>,
    timeout_handler: Mutex<Option<OnSessionTimeoutHandler>>,
}

impl TcpTransportSession {
    pub fn new(stream: TcpStream, id: usize) -> Arc<Self> {
        Arc::new(TcpTransportSession {
            stream: Mutex::new(stream),
            id,
            message_handler: Mutex::new(None),
            receive_handler: Mutex::new(None),
            close_handler: Mutex::new(None),
            error_handler: Mutex::new(None),
            timeout_handler: Mutex::new(None),
        })
    }
}

#[async_trait]
impl TransportSession for TcpTransportSession {
    async fn send_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = self.stream.lock().await;
        let data = packet.to_bytes();
        stream.write_all(&data).await?;
        Ok(())
    }

    async fn receive_packet(self: Arc<Self>) -> Option<Packet> {
        let mut stream = self.stream.lock().await;
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        let packet = Packet::from_bytes(&buf[..n]);
        if let Some(handler) = self.get_receive_handler() {
            let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
            handler.lock().await(context, packet.clone());
        }
        Some(packet)
    }

    async fn process_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Processing packet with ID: {}", packet.message_id);
        let response_packet = Packet::new(42, b"Response test".to_vec());
        self.send_packet(response_packet).await?;
        Ok(())
    }

    async fn close_session(self: Arc<Self>, context: Arc<Context>) {
        let mut stream = self.stream.lock().await;
        let _ = stream.shutdown().await;
        if let Some(handler) = self.get_close_handler() {
            handler.lock().await(context);
        }
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_message_handler(self: Arc<Self>, handler: OnMessageHandler) {
        let mut message_handler = self.message_handler.blocking_lock();
        *message_handler = Some(handler);
    }

    fn get_message_handler(&self) -> Option<OnMessageHandler> {
        let message_handler = self.message_handler.blocking_lock();
        message_handler.clone()
    }

    fn set_receive_handler(self: Arc<Self>, handler: OnReceiveHandler) {
        let mut receive_handler = self.receive_handler.blocking_lock();
        *receive_handler = Some(handler);
    }

    fn get_receive_handler(&self) -> Option<OnReceiveHandler> {
        let receive_handler = self.receive_handler.blocking_lock();
        receive_handler.clone()
    }

    fn set_close_handler(self: Arc<Self>, handler: OnCloseHandler) {
        let mut close_handler = self.close_handler.blocking_lock();
        *close_handler = Some(handler);
    }

    fn get_close_handler(&self) -> Option<OnCloseHandler> {
        let close_handler = self.close_handler.blocking_lock();
        close_handler.clone()
    }

    fn set_error_handler(self: Arc<Self>, handler: OnSessionErrorHandler) {
        let mut error_handler = self.error_handler.blocking_lock();
        *error_handler = Some(handler);
    }

    fn get_error_handler(&self) -> Option<OnSessionErrorHandler> {
        let error_handler = self.error_handler.blocking_lock();
        error_handler.clone()
    }

    fn set_timeout_handler(self: Arc<Self>, handler: OnSessionTimeoutHandler) {
        let mut timeout_handler = self.timeout_handler.blocking_lock();
        *timeout_handler = Some(handler);
    }

    fn get_timeout_handler(&self) -> Option<OnSessionTimeoutHandler> {
        let timeout_handler = self.timeout_handler.blocking_lock();
        timeout_handler.clone()
    }
}