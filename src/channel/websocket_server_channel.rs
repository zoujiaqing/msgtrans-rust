use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
    OnServerTimeoutHandler,
};
use crate::channel::ServerChannel;
use crate::context::Context;
use crate::session::{TransportSession, WebSocketTransportSession};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio::sync::Mutex;

pub struct WebSocketServerChannel {
    host: &'static str,
    port: u16,
    path: &'static str,
}

impl WebSocketServerChannel {
    pub fn new(host: &'static str, port: u16, path: &'static str) -> Self {
        WebSocketServerChannel { host, port, path }
    }
}

#[async_trait::async_trait]
impl ServerChannel for WebSocketServerChannel {
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
        let listener = TcpListener::bind((self.host, self.port)).await.unwrap();

        while let Ok((stream, _)) = listener.accept().await {
            match accept_async(stream).await {
                Ok(ws_stream) => {
                    let session_id = next_id.fetch_add(1, Ordering::SeqCst);

                    let session: Arc<dyn TransportSession + Send + Sync> =
                        WebSocketTransportSession::new(ws_stream, session_id);
                    sessions.lock().await.insert(session_id, Arc::clone(&session));

                    // 触发 OnConnectHandler
                    if let Some(ref handler) = on_connect {
                        let handler = handler.lock().await;
                        handler(Arc::new(Context::new(Arc::clone(&session))));
                    }

                    let on_disconnect_clone = on_disconnect.clone();
                    let on_error_clone = on_error.clone();

                    let session_clone = Arc::clone(&session);
                    tokio::spawn(async move {
                        let session_for_receiving = Arc::clone(&session_clone);
                        if let Err(e) = session_for_receiving.start_receiving().await {
                            // 接收数据时发生错误，触发错误处理
                            if let Some(ref handler) = on_error_clone {
                                let handler = handler.lock().await;
                                handler(e);
                            }
                            // 发生错误后触发断开连接的处理
                            if let Some(ref handler) = on_disconnect_clone {
                                let handler = handler.lock().await;
                                handler(Arc::new(Context::new(Arc::clone(&session_clone))));
                            }
                        }
                    });
                }
                Err(e) => {
                    // 触发 OnErrorHandler
                    if let Some(ref handler) = on_error {
                        let handler = handler.lock().await;
                        handler(Box::new(e));
                    }
                }
            }
        }
    }
}