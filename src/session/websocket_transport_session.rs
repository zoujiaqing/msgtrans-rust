use crate::callbacks::{
    OnMessageHandler, OnReceiveHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::Packet;
use crate::context::Context;
use crate::session::TransportSession;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio::net::TcpStream;
use futures::{StreamExt, SinkExt, stream::SplitSink, stream::SplitStream};
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;
use std::io;

pub struct WebSocketTransportSession {
    send_stream: Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>, // 发送流
    receive_stream: Mutex<SplitStream<WebSocketStream<TcpStream>>>, // 接收流
    id: usize,
    message_handler: Mutex<Option<OnMessageHandler>>,
    close_handler: Mutex<Option<OnCloseHandler>>,
    error_handler: Mutex<Option<OnSessionErrorHandler>>,
    timeout_handler: Mutex<Option<OnSessionTimeoutHandler>>,
}

impl WebSocketTransportSession {
    pub fn new(ws_stream: WebSocketStream<TcpStream>, id: usize) -> Arc<Self> {
        let (send_stream, receive_stream) = ws_stream.split();
        Arc::new(WebSocketTransportSession {
            send_stream: Mutex::new(send_stream),
            receive_stream: Mutex::new(receive_stream),
            id,
            message_handler: Mutex::new(None),
            close_handler: Mutex::new(None),
            error_handler: Mutex::new(None),
            timeout_handler: Mutex::new(None),
        })
    }
}

#[async_trait]
impl TransportSession for WebSocketTransportSession {
    async fn send_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = packet.to_bytes().to_vec();
        let mut send_stream = self.send_stream.lock().await;
        send_stream.send(Message::Binary(data)).await?;
        Ok(())
    }

    async fn start_receiving(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut receive_stream = self.receive_stream.lock().await;

        while let Some(msg) = receive_stream.next().await {
            match msg {
                Ok(Message::Binary(bin)) => {
                    let packet = Packet::from_bytes(&bin);
                    
                    // 如果有接收处理器，调用它
                    if let Some(handler) = self.get_message_handler().await {
                        let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                        handler.lock().await(context, packet.clone());
                    }
                }
                Ok(_) => {
                    // 接收到非二进制消息时处理错误
                    if let Some(handler) = self.get_error_handler().await {
                        let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                        handler.lock().await(context, Box::new(io::Error::new(io::ErrorKind::InvalidData, "Received non-binary message")) as Box<dyn std::error::Error + Send + Sync>);
                    }
                }
                Err(e) => {
                    // 处理WebSocket错误
                    if let Some(handler) = self.get_error_handler().await {
                        let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                        handler.lock().await(context, Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                    }
                    break;
                }
            }
        }
        
        if let Some(handler) = self.get_close_handler().await {
            let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
            handler.lock().await(context);
        }
        
        Ok(())
    }

    async fn close_session(self: Arc<Self>, context: Arc<Context>) {
        let mut send_stream = self.send_stream.lock().await;
        let _ = send_stream.close().await;
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