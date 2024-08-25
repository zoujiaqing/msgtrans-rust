use crate::callbacks::{
    OnMessageHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::{Packet, PacketHeader};
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
    send_stream: Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>,
    receive_stream: Mutex<SplitStream<WebSocketStream<TcpStream>>>,
    id: usize,
    message_handler: Mutex<Option<Arc<Mutex<OnMessageHandler>>>>,
    close_handler: Mutex<Option<Arc<Mutex<OnCloseHandler>>>>,
    error_handler: Mutex<Option<Arc<Mutex<OnSessionErrorHandler>>>>,
    timeout_handler: Mutex<Option<Arc<Mutex<OnSessionTimeoutHandler>>>>,
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
    async fn send(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
                    if bin.len() < 16 {
                        println!("Msg length errorr, length: {}", bin.len());
                        break;
                    }

                    // Check if we have enough data to parse the PacketHeader
                    let header = PacketHeader::from_bytes(&bin[..16]);

                    // Check if the full packet is available
                    let total_length = 16 + header.extend_length as usize + header.message_length as usize;
                    if bin.len() < total_length {
                        println!("Not enough data, length: {}", bin.len());
                        break;
                    }

                    // Parse the full packet
                    let packet = Packet::from_bytes(header, &bin[16..total_length]);
                    if let Some(handler) = self.get_message_handler().await {
                        let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                        handler.lock().await(context, packet.clone());
                    }
                }
                Ok(_) => {
                    if let Some(handler) = self.get_error_handler().await {
                        let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                        handler.lock().await(context, Box::new(io::Error::new(io::ErrorKind::InvalidData, "Received non-binary message")) as Box<dyn std::error::Error + Send + Sync>);
                    }
                }
                Err(e) => {
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