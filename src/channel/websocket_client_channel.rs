use super::ClientChannel;
use crate::packet::Packet;
use crate::callbacks::{OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::MaybeTlsStream;
use futures::stream::StreamExt;
use url::Url;
use futures::sink::SinkExt;
use std::io;

pub struct WebSocketClientChannel {
    ws_stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    address: String,
    port: u16,
    path: String,
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
}

impl WebSocketClientChannel {
    pub fn new(address: &str, port: u16, path: String) -> Self {
        WebSocketClientChannel {
            ws_stream: None,
            address: address.to_string(),
            port,
            path,
            on_reconnect: None,
            on_disconnect: None,
            on_error: None,
            on_send: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for WebSocketClientChannel {
    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = Url::parse(&format!("ws://{}:{}/{}", self.address, self.port, self.path))?;
        match connect_async(url).await {
            Ok((ws_stream, _)) => {
                self.ws_stream = Some(ws_stream);
                if let Some(ref handler) = self.on_reconnect {
                    let handler = handler.lock().await;
                    handler();
                }
                Ok(())
            }
            Err(e) => {
                if let Some(ref handler) = self.on_error {
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
        if let Some(ws_stream) = &mut self.ws_stream {
            let send_result = ws_stream
                .send(Message::Binary(packet.to_bytes().to_vec()))
                .await;
    
            if let Some(ref handler) = self.on_send {
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
    
            if let Some(ref handler) = self.on_error {
                let handler = handler.lock().await;
                handler(err_msg);
            }
    
            Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")))
        }
    }

    async fn receive(&mut self) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ws_stream) = &mut self.ws_stream {
            if let Some(msg) = ws_stream.next().await {
                match msg? {
                    Message::Binary(bin) => {
                        let packet = Packet::from_bytes(&bin);
                        return Ok(Some(packet));
                    }
                    _ => return Ok(None),
                }
            }
        } else {
            if let Some(ref handler) = self.on_error {
                let handler = handler.lock().await;
                handler(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")));
            }
            return Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")));
        }
        Ok(None)
    }

    fn set_reconnect_handler(&mut self, handler: OnReconnectHandler) {
        self.on_reconnect = Some(handler);
    }

    fn set_disconnect_handler(&mut self, handler: OnClientDisconnectHandler) {
        self.on_disconnect = Some(handler);
    }

    fn set_error_handler(&mut self, handler: OnClientErrorHandler) {
        self.on_error = Some(handler);
    }

    fn set_send_handler(&mut self, handler: OnSendHandler) {
        self.on_send = Some(handler);
    }
}