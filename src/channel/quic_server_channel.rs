use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
};
use crate::channel::ServerChannel;
use crate::context::Context;
use crate::session::{QuicTransportSession, TransportSession};
use quinn::{Endpoint, ServerConfig, crypto::rustls::QuicServerConfig};
use rustls::{pki_types::{CertificateDer, PrivateKeyDer}, ServerConfig as RustlsServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Mutex;
use tokio::sync::Notify;

pub struct QuicServerChannel {
    host: String,
    port: u16,
    cert_path: String,
    key_path: String,
    shutdown_notify: Arc<Notify>,
}

impl QuicServerChannel {
    pub fn new(host: &str, port: u16, cert_path: &str, key_path: &str) -> Self {
        let shutdown_notify = Arc::new(Notify::new());
        QuicServerChannel {
            host: host.to_string(),
            port,
            cert_path: cert_path.to_string(),
            key_path: key_path.to_string(),
            shutdown_notify,
        }
    }

    fn load_certs(&self) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error + Send + Sync>> {
        let cert_file = File::open(&self.cert_path)?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Result<Vec<_>, _> = certs(&mut cert_reader).collect();
        Ok(certs?)
    }

    fn load_private_key(&self) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error + Send + Sync>> {
        let key_file = File::open(&self.key_path)?;
        let mut key_reader = BufReader::new(key_file);
        let keys: Result<Vec<_>, _> = pkcs8_private_keys(&mut key_reader).collect();
        let keys = keys?;
        if keys.is_empty() {
            return Err("No private key found".into());
        }
        Ok(PrivateKeyDer::Pkcs8(keys.into_iter().next().unwrap()))
    }
}

#[async_trait::async_trait]
impl ServerChannel for QuicServerChannel {
    async fn start(
        &mut self,
        sessions: Arc<Mutex<HashMap<usize, Arc<dyn TransportSession + Send + Sync>>>>,
        next_id: Arc<AtomicUsize>,
        message_handler: Option<Arc<Mutex<OnMessageHandler>>>,
        connect_handler: Option<Arc<Mutex<OnServerConnectHandler>>>,
        disconnect_handler: Option<Arc<Mutex<OnServerDisconnectHandler>>>,
        error_handler: Option<Arc<Mutex<OnServerErrorHandler>>>,
    ) {
        let address: SocketAddr = format!("{}:{}", self.host, self.port).parse().unwrap();

        // Load TLS certificates and key
        let certs = match self.load_certs() {
            Ok(certs) => certs,
            Err(e) => {
                if let Some(ref handler) = error_handler {
                    let handler = handler.lock().await;
                    handler(e);
                }
                return;
            }
        };

        let key = match self.load_private_key() {
            Ok(key) => key,
            Err(e) => {
                if let Some(ref handler) = error_handler {
                    let handler = handler.lock().await;
                    handler(e);
                }
                return;
            }
        };

        // Configure TLS
        let mut rustls_config = RustlsServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("Failed to configure TLS");
        
        rustls_config.alpn_protocols = vec![b"hq-29".to_vec()];

        let server_config = ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(rustls_config).expect("Failed to create QUIC config")));
        let endpoint = Endpoint::server(server_config, address).expect("Failed to create QUIC endpoint");

        let shutdown_notify = Arc::clone(&self.shutdown_notify);

        // Start the task for accepting connections
        tokio::spawn({
            async move {
                loop {
                    tokio::select! {
                        // Accept new connection
                        connection_result = endpoint.accept() => {
                            if let Some(incoming) = connection_result {
                                match incoming.await {
                                    Ok(connection) => {
                                        let session_id = next_id.fetch_add(1, Ordering::SeqCst);
                                        let session: Arc<dyn TransportSession + Send + Sync> =
                                            QuicTransportSession::new(connection, session_id) as Arc<dyn TransportSession + Send + Sync>;

                                        if let Some(ref handler) = message_handler {
                                            session.clone().set_message_handler(handler.clone()).await;
                                        }

                                        sessions.lock().await.insert(session_id, Arc::clone(&session));

                                        if let Some(ref handler) = connect_handler {
                                            let handler = handler.lock().await;
                                            handler(Arc::new(Context::new(Arc::clone(&session))));
                                        }

                                        let disconnect_handler_clone = disconnect_handler.clone();
                                        let error_handler_clone = error_handler.clone();

                                        tokio::spawn({
                                            let session_clone = Arc::clone(&session);
                                            async move {
                                                if let Err(e) = session_clone.clone().start_receiving().await {
                                                    // Handle error in receiving
                                                    if let Some(ref handler) = error_handler_clone {
                                                        let handler = handler.lock().await;
                                                        handler(e);
                                                    }
                                                    // Handle disconnection
                                                    if let Some(ref handler) = disconnect_handler_clone {
                                                        let handler = handler.lock().await;
                                                        handler(Arc::new(Context::new(session_clone)));
                                                    }
                                                }
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        if let Some(ref handler) = error_handler {
                                            let handler = handler.lock().await;
                                            handler(e.into());
                                        }
                                    }
                                }
                            }
                        }
                        // Shutdown signal received
                        _ = shutdown_notify.notified() => {
                            // Exit the loop and stop accepting connections
                            break;
                        }
                    }
                }
            }
        });
    }

    async fn shutdown(&mut self) {
        // Notify all listeners that the server is shutting down
        self.shutdown_notify.notify_waiters();

        // The server will stop accepting new connections
    }
}