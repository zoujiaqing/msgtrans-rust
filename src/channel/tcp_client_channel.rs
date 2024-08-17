use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler, OnClientMessageHandler,
};
use crate::packet::Packet;
use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::io;

pub struct TcpClientChannel {
    stream: Option<TcpStream>,
    address: String,
    port: u16,
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
    on_message: Option<Arc<Mutex<OnClientMessageHandler>>>,
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
            on_message: None,
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

    fn set_on_message_handler(&mut self, handler: OnClientMessageHandler) {
        self.on_message = Some(Arc::new(Mutex::new(handler)));
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
            let send_result = stream.write_all(&packet.to_bytes()).await;
    
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

    async fn start_receiving(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            loop {
                let mut buf = BytesMut::with_capacity(1024);
                match stream.read_buf(&mut buf).await {
                    Ok(n) if n > 0 => {
                        let packet = Packet::from_bytes(&buf[..n]);
                        if let Some(ref handler_arc) = self.on_message {
                            let handler_arc_guard = handler_arc.lock().await;
                            let handler = handler_arc_guard.lock().await;
                            (handler)(packet);
                        }
                    }
                    Ok(_) => {
                        println!("Connection closed by peer.");
                        if let Some(ref handler_arc) = self.on_disconnect {
                            let handler_arc_guard = handler_arc.lock().await;
                            (handler_arc_guard)();
                        }
                        break;
                    }
                    Err(e) => {
                        if let Some(ref handler_arc) = self.on_error {
                            let handler_arc_guard = handler_arc.lock().await;
                            handler_arc_guard(Box::new(e));
                        }
                        break;
                    }
                }
            }
            Ok(())
        } else {
            if let Some(ref handler_arc) = self.on_error {
                let handler_arc_guard = handler_arc.lock().await;
                handler_arc_guard("No connection established".into());
            }
            Err("No connection established".into())
        }
    }
}