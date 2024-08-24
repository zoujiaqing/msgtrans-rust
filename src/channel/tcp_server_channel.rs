use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
};
use crate::channel::ServerChannel;
use crate::context::Context;
use crate::session::{TransportSession, TcpTransportSession};
use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use tokio::sync::Mutex;
use tokio::net::TcpListener;

pub struct TcpServerChannel {
    host: String,
    port: u16,
}

impl TcpServerChannel {
    pub fn new(host: &str, port: u16) -> Self {
        TcpServerChannel { host: host.to_string(), port }
    }
}

#[async_trait::async_trait]
impl ServerChannel for TcpServerChannel {
    async fn start(
        &mut self,
        sessions: Arc<Mutex<HashMap<usize, Arc<dyn TransportSession + Send + Sync>>>>,
        next_id: Arc<AtomicUsize>,
        message_handler: Option<OnMessageHandler>,
        connect_handler: Option<OnServerConnectHandler>,
        disconnect_handler: Option<OnServerDisconnectHandler>,
        error_handler: Option<OnServerErrorHandler>
    ) {
        let listener = TcpListener::bind((self.host.as_str(), self.port)).await.unwrap();

        while let Ok((stream, _)) = listener.accept().await {
            let session_id = next_id.fetch_add(1, Ordering::SeqCst);
            let session: Arc<dyn TransportSession + Send + Sync> =
                TcpTransportSession::new(stream, session_id);
            sessions.lock().await.insert(session_id, Arc::clone(&session));

            // Set the message handler
            if let Some(ref handler) = message_handler {
                session.clone().set_message_handler(handler.clone()).await;
            }

            // Trigger the ConnectHandler
            if let Some(ref handler) = connect_handler {
                let handler = handler.lock().await;
                handler(Arc::new(Context::new(Arc::clone(&session))));
            }

            // Start processing the session
            let disconnect_handler_clone = disconnect_handler.clone();
            let error_handler_clone = error_handler.clone();

            let session_clone = Arc::clone(&session);
            tokio::spawn(async move {
                let session_for_receiving = Arc::clone(&session_clone);
                if let Err(e) = session_for_receiving.start_receiving().await {
                    // An error occurred while receiving data, trigger the error handler
                    if let Some(ref handler) = error_handler_clone {
                        let handler = handler.lock().await;
                        handler(e);
                    }
                    // Trigger the disconnect handler after an error occurs
                    if let Some(ref handler) = disconnect_handler_clone {
                        let handler = handler.lock().await;
                        handler(Arc::new(Context::new(Arc::clone(&session_clone))));
                    }
                }
            });
        }
    }
}