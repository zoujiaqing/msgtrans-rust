use anyhow::Result;
use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler, OnClientMessageHandler,
};
use crate::packet::{Packet, PacketHeader};
use s2n_quic::client::Connect;
use s2n_quic::connection::Connection;
use s2n_quic::stream::{ReceiveStream, SendStream};
use std::{path::Path, net::SocketAddr};
use bytes::{Buf, Bytes, BytesMut};
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::io;

pub struct QuicClientChannel {
    receive_stream: Option<Arc<Mutex<ReceiveStream>>>,
    send_stream: Option<Arc<Mutex<SendStream>>>,
    connection: Option<Arc<Mutex<Connection>>>,
    host: String,
    port: u16,
    cert_path: &'static str,
    reconnect_handler: Option<Arc<Mutex<OnReconnectHandler>>>,
    disconnect_handler: Option<Arc<Mutex<OnClientDisconnectHandler>>>,
    error_handler: Option<Arc<Mutex<OnClientErrorHandler>>>,
    send_handler: Option<Arc<Mutex<OnSendHandler>>>,
    message_handler: Option<Arc<Mutex<OnClientMessageHandler>>>,
    local_ip: Option<String>,
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
            local_ip: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for QuicClientChannel {
    fn set_reconnect_handler(&mut self, handler: Arc<Mutex<OnReconnectHandler>>) {
        self.reconnect_handler = Some(handler);
    }

    fn set_disconnect_handler(&mut self, handler: Arc<Mutex<OnClientDisconnectHandler>>) {
        self.disconnect_handler = Some(handler);
    }

    fn set_error_handler(&mut self, handler: Arc<Mutex<OnClientErrorHandler>>) {
        self.error_handler = Some(handler);
    }

    fn set_send_handler(&mut self, handler: Arc<Mutex<OnSendHandler>>) {
        self.send_handler = Some(handler);
    }

    fn set_message_handler(&mut self, handler: Arc<Mutex<OnClientMessageHandler>>) {
        self.message_handler = Some(handler);
    }

    fn bind_local_addr(&mut self, ip_addr: String) {
        self.local_ip = Some(ip_addr);
    }

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create the UDP socket, possibly binding to a specific local IP
        let local_addr = match &self.local_ip {
            Some(local_ip) => format!("{}:0", local_ip),
            None => "0.0.0.0:0".to_string(),
        };

        // Initialize the QUIC client
        let client = s2n_quic::Client::builder()
            .with_tls(Path::new(self.cert_path))?
            .with_io(local_addr.as_str())?
            .start()?;

        // Connect to the server
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
            let mut buffer = BytesMut::new();

            loop {
                match receive_stream.receive().await {
                    Ok(Some(data)) => {
                        buffer.extend_from_slice(&data);

                        while buffer.len() >= 16 {
                            // Check if we have enough data to parse the PacketHeader
                            let header = PacketHeader::from_bytes(&buffer[..16]);

                            // Check if the full packet is available
                            let total_length = 16 + header.extend_length as usize + header.message_length as usize;
                            if buffer.len() < total_length {
                                break; // Not enough data, wait for more
                            }

                            // Parse the full packet
                            let packet = Packet::by_header_from_bytes(header, &buffer[16..total_length]);

                            if let Some(ref handler) = message_handler {
                                let handler = handler.lock().await;
                                (*handler)(packet);
                            }

                            // Remove the parsed packet from the buffer
                            buffer.advance(total_length);
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