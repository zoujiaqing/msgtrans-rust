use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler,
};
use crate::packet::Packet;
use bytes::BytesMut;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

pub struct TcpClientChannel {
    stream: Option<TcpStream>,
    address: String,
    port: u16,
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
}

impl TcpClientChannel {
    pub fn new(address: &str, port: u16) -> Self {
        TcpClientChannel {
            stream: None,
            address: address.to_string(),
            port,
            on_reconnect: None,
            on_disconnect: None,
            on_error: None,
            on_send: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for TcpClientChannel {
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

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match TcpStream::connect((&self.address[..], self.port)).await {
            Ok(stream) => {
                self.stream = Some(stream);
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
                Err("Failed to connect".into())
            }
        }
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            if let Some(ref handler) = self.on_send {
                let handler = handler.lock().await;
                handler(packet.clone());
            }
            let data = packet.to_bytes();
            stream.write_all(&data).await?;
            Ok(())
        } else {
            if let Some(ref handler) = self.on_error {
                let handler = handler.lock().await;
                handler("No connection established".into());
            }
            Err("No connection established".into())
        }
    }

    async fn receive(
        &mut self,
    ) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            let mut buf = BytesMut::with_capacity(1024);
            match stream.read_buf(&mut buf).await {
                Ok(n) if n > 0 => Ok(Some(Packet::from_bytes(&buf[..n]))),
                Ok(_) => Ok(None),
                Err(e) => {
                    if let Some(ref handler) = self.on_error {
                        let handler = handler.lock().await;
                        handler(Box::new(e));
                    }
                    Err("Failed to receive packet".into())
                }
            }
        } else {
            if let Some(ref handler) = self.on_error {
                let handler = handler.lock().await;
                handler("No connection established".into());
            }
            Err("No connection established".into())
        }
    }
}
