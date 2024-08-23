use anyhow::Result;
use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler, OnClientMessageHandler,
};
use crate::packet::Packet;
use s2n_quic::client::Connect;
use s2n_quic::connection::Connection;
use s2n_quic::stream::{ReceiveStream, SendStream};
use std::{path::Path, net::SocketAddr};
use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use std::sync::{Arc};
use tokio::sync::Mutex;
use std::io;

pub struct QuicClientChannel {
    receive_stream: Option<Arc<Mutex<ReceiveStream>>>,
    send_stream: Option<Arc<Mutex<SendStream>>>,
    connection: Option<Arc<Mutex<Connection>>>,
    host: String,
    port: u16,
    cert_path: &'static str,
    reconnect_handler: Option<OnReconnectHandler>,
    disconnect_handler: Option<OnClientDisconnectHandler>,
    error_handler: Option<OnClientErrorHandler>,
    send_handler: Option<OnSendHandler>,
    message_handler: Option<OnClientMessageHandler>,
}

impl QuicClientChannel {
    pub fn new(host: &str, port: u16, cert_path: &'static str) -> Self {
        QuicClientChannel {
            receive_stream: None,
            send_stream: None,
            connection: None,
            host: host.to_string(),
            port,
            cert_path,
            reconnect_handler: None,
            disconnect_handler: None,
            error_handler: None,
            send_handler: None,
            message_handler: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for QuicClientChannel {
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
        self.message_handler = Some(handler);
    }

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = s2n_quic::Client::builder()
            .with_tls(Path::new(self.cert_path))?
            .with_io("0.0.0.0:0")?
            .start()?;

        let addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let connect = Connect::new(addr).with_server_name("localhost");
        let mut connection = client.connect(connect).await?;

        connection.keep_alive(true)?;

        let (receive_stream, send_stream) = connection.open_bidirectional_stream().await?.split();

        self.receive_stream = Some(Arc::new(Mutex::new(receive_stream)));
        self.send_stream = Some(Arc::new(Mutex::new(send_stream)));
        self.connection = Some(Arc::new(Mutex::new(connection)));

        if let Some(ref handler) = self.reconnect_handler {
            let handler = handler.lock().await;
            (*handler)();
        }

        let receive_stream_clone = Arc::clone(self.receive_stream.as_ref().unwrap());
        let message_handler = self.message_handler.clone();
        let disconnect_handler = self.disconnect_handler.clone();
        let error_handler = self.error_handler.clone();

        tokio::spawn(async move {
            let mut receive_stream = receive_stream_clone.lock().await;

            loop {
                match receive_stream.receive().await {
                    Ok(Some(data)) => {
                        if let Some(ref handler) = message_handler {
                            let packet = Packet::from_bytes(&data);
                            let handler = handler.lock().await;
                            (*handler)(packet);
                        }
                    }
                    Ok(None) => {
                        println!("Stream closed");
                        if let Some(ref handler) = disconnect_handler {
                            let handler = handler.lock().await;
                            (*handler)();
                        }
                        break;
                    }
                    Err(e) => {
                        eprintln!("Error reading from stream: {:?}", e);
                        if let Some(ref handler) = error_handler {
                            let handler = handler.lock().await;
                            (*handler)(Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                        }
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref send_stream) = self.send_stream {
            let mut send_stream = send_stream.lock().await;
            let data = Bytes::from(packet.to_bytes());
            send_stream.write_all(&data).await?;

            if let Some(ref handler) = self.send_handler {
                let handler = handler.lock().await;
                (*handler)(packet.clone(), Ok(()));
            }

            Ok(())
        } else {
            let err_msg: Box<dyn std::error::Error + Send + Sync> = "No connection established".into();

            if let Some(ref handler) = self.error_handler {
                let handler = handler.lock().await;
                (*handler)(err_msg);
            }

            Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")))
        }
    }
}