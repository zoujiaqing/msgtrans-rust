use anyhow::Result;
use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler, OnClientMessageHandler,
};
use crate::packet::{Packet, PacketHeader};
use quinn::{Endpoint, ClientConfig, Connection, RecvStream, SendStream};
use rustls::{pki_types::{CertificateDer, ServerName, UnixTime}, ClientConfig as RustlsClientConfig, RootCertStore, 
            client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier}, 
            DigitallySignedStruct, SignatureScheme, CertificateError};
use rustls_pemfile::certs;
use std::{path::Path, net::SocketAddr, fs::File, io::BufReader};
use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::io;

#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

pub struct QuicClientChannel {
    receive_stream: Option<Arc<Mutex<RecvStream>>>,
    send_stream: Option<Arc<Mutex<SendStream>>>,
    connection: Option<Arc<Mutex<Connection>>>,
    host: String,
    port: u16,
    cert_path: String,
    reconnect_handler: Option<Arc<OnReconnectHandler>>,
    disconnect_handler: Option<Arc<OnClientDisconnectHandler>>,
    error_handler: Option<Arc<OnClientErrorHandler>>,
    send_handler: Option<Arc<OnSendHandler>>,
    message_handler: Option<Arc<OnClientMessageHandler>>,
    local_ip: Option<String>,
}

impl QuicClientChannel {
    pub fn new(host: &str, port: u16, cert_path: &str) -> Self {
        QuicClientChannel {
            receive_stream: None,
            send_stream: None,
            connection: None,
            host: host.to_string(),
            port,
            cert_path: cert_path.to_string(),
            reconnect_handler: None,
            disconnect_handler: None,
            error_handler: None,
            send_handler: None,
            message_handler: None,
            local_ip: None,
        }
    }

    fn load_certs(&self) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error + Send + Sync>> {
        let cert_file = File::open(&self.cert_path)?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Result<Vec<_>, _> = certs(&mut cert_reader).collect();
        Ok(certs?)
    }
}

#[async_trait::async_trait]
impl ClientChannel for QuicClientChannel {
    fn set_reconnect_handler(&mut self, handler: Arc<OnReconnectHandler>) {
        self.reconnect_handler = Some(handler);
    }

    fn set_disconnect_handler(&mut self, handler: Arc<OnClientDisconnectHandler>) {
        self.disconnect_handler = Some(handler);
    }

    fn set_error_handler(&mut self, handler: Arc<OnClientErrorHandler>) {
        self.error_handler = Some(handler);
    }

    fn set_send_handler(&mut self, handler: Arc<OnSendHandler>) {
        self.send_handler = Some(handler);
    }

    fn set_message_handler(&mut self, handler: Arc<OnClientMessageHandler>) {
        self.message_handler = Some(handler);
    }

    fn bind_local_addr(&mut self, ip_addr: String) {
        self.local_ip = Some(ip_addr);
    }

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create the local address
        let local_addr: SocketAddr = match &self.local_ip {
            Some(local_ip) => format!("{}:0", local_ip).parse()?,
            None => "0.0.0.0:0".parse()?,
        };

        // Configure TLS with dangerous config (skip certificate verification for testing)
        let mut rustls_config = RustlsClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();
        
        rustls_config.alpn_protocols = vec![b"hq-29".to_vec()];

        let client_config = ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)?));

        // Create endpoint
        let mut endpoint = Endpoint::client(local_addr)?;
        endpoint.set_default_client_config(client_config);

        // Connect to the server
        let server_addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let connection = endpoint.connect(server_addr, &self.host)?.await?;

        let (send_stream, recv_stream) = connection.open_bi().await?;

        self.receive_stream = Some(Arc::new(Mutex::new(recv_stream)));
        self.send_stream = Some(Arc::new(Mutex::new(send_stream)));
        self.connection = Some(Arc::new(Mutex::new(connection)));

        if let Some(ref handler) = self.reconnect_handler {
            (*handler)();
        }

        let receive_stream_clone = Arc::clone(self.receive_stream.as_ref().unwrap());
        let message_handler = self.message_handler.clone();
        let disconnect_handler = self.disconnect_handler.clone();
        let error_handler = self.error_handler.clone();

        tokio::spawn(async move {
            let mut receive_stream = receive_stream_clone.lock().await;
            let mut buffer = BytesMut::new();
            let mut temp_buffer = [0u8; 1024];

            loop {
                match receive_stream.read(&mut temp_buffer).await {
                    Ok(Some(n)) => {
                        if n > 0 {
                            buffer.extend_from_slice(&temp_buffer[..n]);

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
                                    (*handler)(packet);
                                }

                                // Remove the parsed packet from the buffer
                                buffer.advance(total_length);
                            }
                        } else {
                            // Stream closed (0 bytes read)
                            println!("Stream closed");
                            if let Some(ref handler) = disconnect_handler {
                                (*handler)();
                            }
                            break;
                        }
                    }
                    Ok(None) => {
                        println!("Stream closed");
                        if let Some(ref handler) = disconnect_handler {
                            (*handler)();
                        }
                        break;
                    }
                    Err(e) => {
                        eprintln!("Error reading from stream: {:?}", e);
                        if let Some(ref handler) = error_handler {
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
            let data = packet.to_bytes();
            send_stream.write_all(&data).await?;

            if let Some(ref handler) = self.send_handler {
                (*handler)(packet.clone(), Ok(()));
            }

            Ok(())
        } else {
            let err_msg: Box<dyn std::error::Error + Send + Sync> = "No connection established".into();

            if let Some(ref handler) = self.error_handler {
                (*handler)(err_msg);
            }

            Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")))
        }
    }
}