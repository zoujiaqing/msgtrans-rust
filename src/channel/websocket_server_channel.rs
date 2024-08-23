use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
};
use crate::channel::ServerChannel;
use crate::context::Context;
use crate::session::{TransportSession, WebSocketTransportSession};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_hdr_async;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::http::{Request, Response};

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
        connect_handler: Option<OnServerConnectHandler>,
        disconnect_handler: Option<OnServerDisconnectHandler>,
        error_handler: Option<OnServerErrorHandler>,
    ) {
        let listener = TcpListener::bind((self.host, self.port)).await.unwrap();

        while let Ok((stream, _)) = listener.accept().await {
            let path = self.path.to_string();
            match accept_hdr_async(stream, |req: &Request<()>, response| {
                println!("req.path: {}, server.path: {}", req.uri().path(), path);
                if req.uri().path() == path {
                    Ok(response)
                } else {
                    Err(Response::builder()
                        .status(404)
                        .body(None)
                        .unwrap())
                }
            }).await {
                Ok(ws_stream) => {
                    let session_id = next_id.fetch_add(1, Ordering::SeqCst);

                    let session: Arc<dyn TransportSession + Send + Sync> =
                        WebSocketTransportSession::new(ws_stream, session_id);
                    sessions.lock().await.insert(session_id, Arc::clone(&session));

                    // 设置消息处理器
                    if let Some(ref handler) = message_handler {
                        session.clone().set_message_handler(handler.clone()).await;
                    }

                    // 触发 OnConnectHandler
                    if let Some(ref handler) = connect_handler {
                        let handler = handler.lock().await;
                        handler(Arc::new(Context::new(Arc::clone(&session))));
                    }

                    let disconnect_handler_clone = disconnect_handler.clone();
                    let error_handler_clone = error_handler.clone();

                    let session_clone = Arc::clone(&session);
                    tokio::spawn(async move {
                        let session_for_receiving = Arc::clone(&session_clone);
                        if let Err(e) = session_for_receiving.start_receiving().await {
                            // 接收数据时发生错误，触发错误处理
                            if let Some(ref handler) = error_handler_clone {
                                let handler = handler.lock().await;
                                handler(e);
                            }
                            // 发生错误后触发断开连接的处理
                            if let Some(ref handler) = disconnect_handler_clone {
                                let handler = handler.lock().await;
                                handler(Arc::new(Context::new(Arc::clone(&session_clone))));
                            }
                        }
                    });
                }
                Err(e) => {
                    // 触发 OnErrorHandler
                    if let Some(ref handler) = error_handler {
                        let handler = handler.lock().await;
                        handler(Box::new(e));
                    }
                }
            }
        }
    }
}