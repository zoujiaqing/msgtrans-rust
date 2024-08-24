use anyhow::Result;
use super::ClientChannel;
use crate::packet::Packet;
use crate::callbacks::{
    OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler, OnClientMessageHandler,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::MaybeTlsStream;
use futures::stream::StreamExt;
use futures::sink::SinkExt;
use url::Url;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::io;

pub struct WebSocketClientChannel {
    send_stream: Option<Arc<Mutex<futures::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>>,
    receive_stream: Option<Arc<Mutex<futures::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>>,
    address: String,
    port: u16,
    path: String,
    reconnect_handler: Option<OnReconnectHandler>,
    disconnect_handler: Option<OnClientDisconnectHandler>,
    error_handler: Option<OnClientErrorHandler>,
    send_handler: Option<OnSendHandler>,
    message_handler: Option<Arc<Mutex<OnClientMessageHandler>>>,
}

impl WebSocketClientChannel {
    pub fn new(address: &str, port: u16, path: String) -> Self {
        WebSocketClientChannel {
            send_stream: None,
            receive_stream: None,
            address: address.to_string(),
            port,
            path,
            reconnect_handler: None,
            disconnect_handler: None,
            error_handler: None,
            send_handler: None,
            message_handler: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for WebSocketClientChannel {
    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = Url::parse(&format!("ws://{}:{}{}", self.address, self.port, self.path))?;
        match connect_async(url).await {
            Ok((ws_stream, _)) => {
                let (send_stream, receive_stream) = ws_stream.split();
                self.send_stream = Some(Arc::new(Mutex::new(send_stream)));
                self.receive_stream = Some(Arc::new(Mutex::new(receive_stream)));
                
                if let Some(ref handler) = self.reconnect_handler {
                    let handler = handler.lock().await;
                    handler();
                }

                let receive_stream = self.receive_stream.clone();
                let message_handler = self.message_handler.clone();
                let disconnect_handler = self.disconnect_handler.clone();
                let error_handler = self.error_handler.clone();

                tokio::spawn(async move {
                    if let Some(receive_stream) = receive_stream {
                        let mut receive_stream = receive_stream.lock().await;
                        while let Some(msg) = receive_stream.next().await {
                            match msg {
                                Ok(Message::Binary(bin)) => {
                                    if let Some(ref handler_arc) = message_handler {
                                        let handler_arc_guard = handler_arc.lock().await;
                                        let handler = handler_arc_guard.lock().await;
                                        let packet = Packet::from_bytes(&bin);
                                        (*handler)(packet);
                                    }
                                }
                                Ok(_) => continue,
                                Err(e) => {
                                    if let Some(handler_arc) = &error_handler {
                                        let handler = handler_arc.lock().await;
                                        (handler)(Box::new(e));
                                    }
                                    break;
                                }
                            }
                        }

                        if let Some(handler_arc) = &disconnect_handler {
                            let handler = handler_arc.lock().await;
                            (handler)();
                        }
                    }
                });

                Ok(())
            }
            Err(e) => {
                if let Some(ref handler) = self.error_handler {
                    let handler = handler.lock().await;
                    handler(Box::new(e));
                }
                Err(Box::new(io::Error::new(io::ErrorKind::Other, "Failed to connect")))
            }
        }
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(send_stream) = &self.send_stream {
            let mut send_stream = send_stream.lock().await;
            let send_result = send_stream
                .send(Message::Binary(packet.to_bytes().to_vec()))
                .await;

            if let Some(ref handler) = self.send_handler {
                let handler = handler.lock().await;
                let send_result_for_handler = send_result
                    .as_ref()
                    .map(|_| ())
                    .map_err(|e| Box::new(io::Error::new(io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>);
                handler(packet.clone(), send_result_for_handler);
            }

            send_result.map(|_| ()).map_err(|e| Box::new(io::Error::new(io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            let err_msg: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established"));

            if let Some(ref handler) = self.error_handler {
                let handler = handler.lock().await;
                handler(err_msg);
            }

            Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")))
        }
    }

    fn set_reconnect_handler(&mut self, handler: OnReconnectHandler) {
        self.reconnect_handler = Some(handler);
    }

    fn set_disconnect_handler(&mut self, handler: OnClientDisconnectHandler) {
        self.disconnect_handler = Some(handler);
    }

    fn set_error_handler(&mut self, handler: OnClientErrorHandler) {
        self.error_handler = Some(handler);
    }

    fn set_send_handler(&mut self, handler: OnSendHandler) {
        self.send_handler = Some(handler);
    }

    fn set_message_handler(&mut self, handler: OnClientMessageHandler) {
        self.message_handler = Some(Arc::new(Mutex::new(handler)));
    }
}