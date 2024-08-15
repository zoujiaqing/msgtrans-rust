use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler,
};
use crate::packet::Packet;
use s2n_quic::client::Connect;
use s2n_quic::connection::Connection;
use s2n_quic::stream::BidirectionalStream;
use std::{path::Path, net::SocketAddr};
use bytes::Bytes;

pub struct QuicClientChannel {
    connection: Option<Connection>,
    host: String,
    port: u16,
    cert_path: &'static str,
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
}

impl QuicClientChannel {
    pub fn new(host: &str, port: u16, cert_path: &'static str) -> Self {
        QuicClientChannel {
            connection: None,
            host: host.to_string(),
            port,
            cert_path,
            on_reconnect: None,
            on_disconnect: None,
            on_error: None,
            on_send: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for QuicClientChannel {
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
        println!("Connecting ...");
        let client = s2n_quic::Client::builder()
            .with_tls(Path::new(self.cert_path))?
            .with_io("0.0.0.0:0")?
            .start()?;

        let addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let connect = Connect::new(addr).with_server_name("localhost");
        let connection = client.connect(connect).await?;
        println!("Connected!");

        self.connection = Some(connection);

        if let Some(ref handler) = self.on_reconnect {
            let handler = handler.lock().await;
            handler();
        }

        Ok(())
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(connection) = &mut self.connection {
            let mut stream: BidirectionalStream = connection.open_bidirectional_stream().await?;

            if let Some(ref handler) = self.on_send {
                let handler = handler.lock().await;
                handler(packet.clone());
            }

            let data = Bytes::from(packet.to_bytes());
            stream.send(data).await?;
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
        if let Some(connection) = &mut self.connection {
            match connection.accept_bidirectional_stream().await? {
                Some(mut stream) => {
                    while let Ok(Some(data)) = stream.receive().await {
                        if !data.is_empty() {
                            return Ok(Some(Packet::from_bytes(&data)));
                        }
                    }
                    Ok(None)
                }
                None => Ok(None),
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
