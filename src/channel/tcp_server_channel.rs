use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
    OnServerTimeoutHandler,
};
use crate::channel::ServerChannel;
use crate::context::Context;
use crate::session::{TransportSession, TcpTransportSession};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use std::sync::atomic::Ordering;

pub struct TcpServerChannel {
    host: String,
    port: u16,
}

impl TcpServerChannel {
    pub fn new(host: &str, port: u16) -> Self {
        TcpServerChannel { host: host.to_string(), port: port }
    }
}

#[async_trait::async_trait]
impl ServerChannel for TcpServerChannel {
    async fn start(
        &mut self,
        sessions: Arc<Mutex<HashMap<usize, Arc<dyn TransportSession + Send + Sync>>>>,
        next_id: Arc<AtomicUsize>,
        message_handler: Option<OnMessageHandler>,
        on_connect: Option<OnServerConnectHandler>,
        on_disconnect: Option<OnServerDisconnectHandler>,
        on_error: Option<OnServerErrorHandler>,
        on_timeout: Option<OnServerTimeoutHandler>,
    ) {

        let listener = TcpListener::bind(("0.0.0.0", self.port)).await.unwrap();

        tokio::spawn( async {

        });
        while let Ok((stream, _)) = listener.accept().await {
            let session_id = next_id.fetch_add(1, Ordering::SeqCst);

            let session: Arc<dyn TransportSession + Send + Sync> =
                TcpTransportSession::new(stream, session_id);
            sessions.lock().await.insert(session_id, Arc::clone(&session));

            // 触发 OnConnectHandler
            if let Some(ref handler) = on_connect {
                let handler = handler.lock().await;
                handler(Arc::new(Context::new(Arc::clone(&session))));
            }

            // 开始处理会话
            let message_handler_clone = message_handler.clone();
            let on_disconnect_clone = on_disconnect.clone();
            let on_error_clone = on_error.clone();
            let session_clone = Arc::clone(&session); // 克隆 Arc 以避免移动
            tokio::spawn(async move {
                while let Some(packet) = session_clone.clone().receive_packet().await {
                    if let Some(ref handler) = message_handler_clone {
                        let handler = handler.lock().await;
                        handler(Arc::new(Context::new(Arc::clone(&session_clone))), packet.clone());
                    }
                    if let Err(e) = session_clone.clone().process_packet(packet).await {
                        if let Some(ref handler) = on_error_clone {
                            let handler = handler.lock().await;
                            handler(e);
                        }
                    }
                }
                // 触发 OnDisconnectHandler
                if let Some(ref handler) = on_disconnect_clone {
                    let handler = handler.lock().await;
                    handler(Arc::new(Context::new(Arc::clone(&session_clone))));
                }
            });
        }
    }
}