use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
    OnServerTimeoutHandler,
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
    server: Option<Server>, // 使用 Option 来表示可选的 Server
}

impl QuicServerChannel {
    pub fn new(host: &str, port: u16, cert_path: &str, key_path: &str) -> Self {
        QuicServerChannel {
            host: host.to_string(),
            port,
            cert_path: cert_path.to_string(),
            key_path: key_path.to_string(),
            server: None, // 初始化为 None
        }
    }
}

#[async_trait::async_trait]
impl ServerChannel for QuicServerChannel {
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
            let session: Arc<dyn TransportSession + Send + Sync> =
                QuicTransportSession::new(connection, session_id);

            println!("session_id: {}", session_id);

            sessions.lock().await.insert(session_id, Arc::clone(&session));

            // 触发 OnConnectHandler
            if let Some(ref handler) = on_connect {
                let handler = handler.lock().await;
                handler(Arc::new(Context::new(Arc::clone(&session))));
            }

            let message_handler_clone = message_handler.clone();
            let on_disconnect_clone = on_disconnect.clone();
            let on_error_clone = on_error.clone();

            tokio::spawn({
                let session_clone = Arc::clone(&session);
                async move {
                    if let Err(e) = session_clone.clone().start_receiving().await {
                        // 接收数据时发生错误，触发错误处理
                        if let Some(ref handler) = on_error_clone {
                            let handler = handler.lock().await;
                            handler(e); // 不再需要额外的 Box 包装
                        }
                        // 发生错误后退出，可能表示连接关闭
                        if let Some(ref handler) = on_disconnect_clone {
                            let handler = handler.lock().await;
                            handler(Arc::new(Context::new(Arc::clone(&session_clone))));
                        }
                    }
                }
            });
        }
    }
}