use crate::callbacks::{
    OnMessageHandler, OnReceiveHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::Packet;
use crate::context::Context;
use crate::session::TransportSession;
use s2n_quic::connection::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;
use bytes::Bytes;

pub struct QuicTransportSession {
    connection: Mutex<Connection>, // 使用 Mutex 保护 Connection
    id: usize,
    message_handler: Mutex<Option<OnMessageHandler>>, // 使用 Mutex 保护处理器
    receive_handler: Mutex<Option<OnReceiveHandler>>,
    close_handler: Mutex<Option<OnCloseHandler>>,
    error_handler: Mutex<Option<OnSessionErrorHandler>>,
    timeout_handler: Mutex<Option<OnSessionTimeoutHandler>>,
}

impl QuicTransportSession {
    pub fn new(connection: Connection, id: usize) -> Arc<Self> {
        Arc::new(QuicTransportSession {
            connection: Mutex::new(connection),
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
impl TransportSession for QuicTransportSession {
    async fn send_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut connection = self.connection.lock().await;
        let mut stream = connection.open_bidirectional_stream().await?;
        let data = packet.to_bytes();
        stream.send(Bytes::from(data)).await?;
        Ok(())
    }

    async fn receive_packet(self: Arc<Self>) -> Option<Packet> {
        let mut connection = self.connection.lock().await;
        if let Ok(Some(mut stream)) = connection.accept_bidirectional_stream().await {
            if let Ok(Some(data)) = stream.receive().await {
                let packet = Packet::from_bytes(&data);
                if let Some(handler) = self.get_receive_handler().await {
                    let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                    handler.lock().await(context, packet.clone());
                }
                return Some(packet);
            }
        }
        None
    }

    async fn process_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Processing QUIC packet with ID: {}", packet.message_id);
        let response_packet = Packet::new(42, b"QUIC response test".to_vec());
        self.send_packet(response_packet).await?;
        Ok(())
    }

    async fn close_session(self: Arc<Self>, context: Arc<Context>) {
        let connection = self.connection.lock().await;
        
        const MY_ERROR_CODE: u32 = 99;
        connection.close(MY_ERROR_CODE.into());
    
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

    async fn set_receive_handler(self: Arc<Self>, handler: OnReceiveHandler) {
        let mut receive_handler = self.receive_handler.lock().await;
        *receive_handler = Some(handler);
    }

    async fn get_receive_handler(&self) -> Option<OnReceiveHandler> {
        let receive_handler = self.receive_handler.lock().await;
        receive_handler.clone()
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