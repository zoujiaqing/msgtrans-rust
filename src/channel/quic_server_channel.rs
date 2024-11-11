use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
};
use crate::channel::ServerChannel;
use crate::context::Context;
use crate::session::{QuicTransportSession, TransportSession};
use s2n_quic::Server;
use std::collections::HashMap;
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

        // Start the QUIC server
        let server = Server::builder()
            .with_tls((Path::new(&self.cert_path), Path::new(&self.key_path)))
            .expect("Failed to configure TLS")
            .with_io(address.as_str())
            .expect("Failed to configure IO")
            .start()
            .expect("Failed to start QUIC server");

        // Use Arc<Mutex<Server>> to allow sharing and mutability
        let server_arc = Arc::new(Mutex::new(Some(server)));
        let shutdown_notify = Arc::clone(&self.shutdown_notify);

        // Start the task for accepting connections
        tokio::spawn({
            let server_arc_clone = Arc::clone(&server_arc);
            let shutdown_notify_clone = Arc::clone(&shutdown_notify);

            async move {
                loop {
                    tokio::select! {
                        // Accept new connection, scope locked server separately
                        connection = async {
                            let mut locked_server = server_arc_clone.lock().await;
                            locked_server.as_mut().unwrap().accept().await
                        } => {
                            if let Some(connection) = connection {
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
                        }
                        // Shutdown signal received
                        _ = shutdown_notify_clone.notified() => {
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