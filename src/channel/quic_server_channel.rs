use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
};
use crate::channel::ServerChannel;
use crate::context::Context;
use crate::session::{TransportSession, QuicTransportSession};
use s2n_quic::Server;
use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use tokio::sync::Mutex;
use std::path::Path;

pub struct QuicServerChannel {
    host: String,
    port: u16,
    cert_path: String,
    key_path: String,
    server: Option<Server>,
}

impl QuicServerChannel {
    pub fn new(host: &str, port: u16, cert_path: &str, key_path: &str) -> Self {
        QuicServerChannel {
            host: host.to_string(),
            port,
            cert_path: cert_path.to_string(),
            key_path: key_path.to_string(),
            server: None,
        }
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
        let address = format!("{}:{}", self.host, self.port);
        self.server = Some(Server::builder()
            .with_tls((Path::new(&self.cert_path), Path::new(&self.key_path)))
            .expect("Failed to configure TLS")
            .with_io(address.as_str())
            .expect("Failed to configure IO")
            .start()
            .expect("Failed to start QUIC server"));
        
        while let Some(connection) = self.server.as_mut().unwrap().accept().await {
            let session_id = next_id.fetch_add(1, Ordering::SeqCst);
            let session: Arc<dyn TransportSession + Send + Sync> = QuicTransportSession::new(connection, session_id) as Arc<dyn TransportSession + Send + Sync>;

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
                        // An error occurred while receiving data, triggering the error handler
                        if let Some(ref handler) = error_handler_clone {
                            let handler = handler.lock().await;
                            handler(e);
                        }
                        // Exit after an error occurs, possibly indicating the connection is closed
                        if let Some(ref handler) = disconnect_handler_clone {
                            let handler = handler.lock().await;
                            handler(Arc::new(Context::new(session_clone)));
                        }
                    }
                }
            });
        }
    }
}