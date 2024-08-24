use anyhow::Result;
use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler, OnClientMessageHandler,
};
use crate::packet::Packet;
use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::io;

pub struct TcpClientChannel {
    read_half: Option<Arc<Mutex<ReadHalf<TcpStream>>>>,
    write_half: Option<Arc<Mutex<WriteHalf<TcpStream>>>>,
    address: String,
    port: u16,
    reconnect_handler: Option<OnReconnectHandler>,
    disconnect_handler: Option<OnClientDisconnectHandler>,
    error_handler: Option<OnClientErrorHandler>,
    send_handler: Option<OnSendHandler>,
    message_handler: Option<Arc<Mutex<OnClientMessageHandler>>>,
}

impl TcpClientChannel {
    pub fn new(address: &str, port: u16) -> Self {
        TcpClientChannel {
            read_half: None,
            write_half: None,
            address: address.to_string(),
            port,
            reconnect_handler: None,
            disconnect_handler: None,
            error_handler: None,
            send_handler: None,
            message_handler: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for TcpClientChannel {
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

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match TcpStream::connect((&self.address[..], self.port)).await {
            Ok(stream) => {
                let (read_half, write_half) = tokio::io::split(stream);
                self.read_half = Some(Arc::new(Mutex::new(read_half)));
                self.write_half = Some(Arc::new(Mutex::new(write_half)));
                
                if let Some(ref handler) = self.reconnect_handler {
                    let handler = handler.lock().await;
                    handler();
                }

                let read_half_clone = Arc::clone(self.read_half.as_ref().unwrap());
                let message_handler = self.message_handler.clone();
                let disconnect_handler = self.disconnect_handler.clone();
                let error_handler = self.error_handler.clone();

                tokio::spawn(async move {
                    let mut stream = read_half_clone.lock().await;
                    loop {
                        let mut buf = BytesMut::with_capacity(1024);
                        match stream.read_buf(&mut buf).await {
                            Ok(n) if n > 0 => {
                                let packet = Packet::from_bytes(&buf[..n]);

                                if let Some(ref handler_arc) = message_handler {
                                    let handler_arc_guard = handler_arc.lock().await;
                                    let handler = handler_arc_guard.lock().await;
                                    (*handler)(packet);
                                }
                            }
                            Ok(_) => {
                                println!("Connection closed by peer.");
                                if let Some(ref handler_arc) = disconnect_handler {
                                    let handler = handler_arc.lock().await;
                                    (handler)();
                                }
                                break;
                            }
                            Err(e) => {
                                if let Some(ref handler_arc) = error_handler {
                                    let handler = handler_arc.lock().await;
                                    handler(Box::new(e));
                                }
                                break;
                            }
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
                Err("Failed to connect".into())
            }
        }
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(write_half) = &self.write_half {
            let mut stream = write_half.lock().await;
            let send_result = stream.write_all(&packet.to_bytes()).await;

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
}