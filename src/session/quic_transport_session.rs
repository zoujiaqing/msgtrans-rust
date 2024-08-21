use crate::callbacks::{
    OnMessageHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::Packet;
use crate::context::Context;
use s2n_quic::connection::Connection;
use s2n_quic::stream::{ReceiveStream, SendStream};
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;
use bytes::Bytes;
use crate::session::TransportSession;

pub struct QuicTransportSession {
    connection: Arc<Mutex<Connection>>,
    receive_stream: Arc<Mutex<Option<ReceiveStream>>>,
    send_stream: Arc<Mutex<Option<SendStream>>>,
    id: usize,
    message_handler: Mutex<Option<OnMessageHandler>>,
    close_handler: Mutex<Option<OnCloseHandler>>,
    error_handler: Mutex<Option<OnSessionErrorHandler>>,
    timeout_handler: Mutex<Option<OnSessionTimeoutHandler>>,
}

impl QuicTransportSession {
    pub fn new(connection: Connection, id: usize) -> Arc<Self> {
        Arc::new(QuicTransportSession {
            connection: Arc::new(Mutex::new(connection)),
            receive_stream: Arc::new(Mutex::new(None)),
            send_stream: Arc::new(Mutex::new(None)),
            id,
            message_handler: Mutex::new(None),
            close_handler: Mutex::new(None),
            error_handler: Mutex::new(None),
            timeout_handler: Mutex::new(None),
        })
    }
}

#[async_trait]
impl TransportSession for QuicTransportSession {
    async fn send_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream_guard = self.send_stream.lock().await;
        if let Some(ref mut send_stream) = *stream_guard {
            let data = packet.to_bytes();
            send_stream.send(Bytes::from(data)).await?;
        } else {
            return Err("Send stream is not available".into());
        }
        Ok(())
    }
    
    async fn start_receiving(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut connection_guard = self.connection.lock().await;
        if let Some(stream) = connection_guard.accept_bidirectional_stream().await? {
            let (receive_stream, send_stream) = stream.split();

            // 更新 receive_stream 和 send_stream 字段
            *self.receive_stream.lock().await = Some(receive_stream);
            *self.send_stream.lock().await = Some(send_stream);
        }

        // 处理接收数据的逻辑
        if let Some(ref mut stream) = *self.receive_stream.lock().await {
            while let Some(data) = stream.receive().await? {
                let packet = Packet::from_bytes(&data);

                if let Some(handler) = self.get_message_handler().await {
                    let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                    handler.lock().await(context, packet);
                }
            }
        } else {
            return Err("Receive stream is not available".into());
        }
    
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