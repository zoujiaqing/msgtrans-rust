use super::ServerChannel;
use crate::session::{TransportSession, WebSocketTransportSession};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

pub struct WebSocketServerChannel {
    port: u16,
    path: &'static str,
}

impl WebSocketServerChannel {
    pub fn new(port: u16, path: &'static str) -> Self {
        WebSocketServerChannel { port, path }
    }
}

#[async_trait::async_trait]
impl ServerChannel for WebSocketServerChannel {
    async fn start(
        &mut self,
        sessions: Arc<Mutex<HashMap<usize, Arc<Mutex<dyn TransportSession + Send + Sync>>>>>,  // 使用 tokio::sync::Mutex
        next_id: Arc<AtomicUsize>,
    ) {
        let listener = TcpListener::bind(("0.0.0.0", self.port)).await.unwrap();
        while let Ok((stream, _)) = listener.accept().await {
            let ws_stream = accept_async(stream).await.unwrap();
            let session_id = next_id.fetch_add(1, Ordering::SeqCst);

            let session: Arc<Mutex<dyn TransportSession + Send + Sync>> =
                Arc::new(Mutex::new(WebSocketTransportSession::new(ws_stream, session_id)));
            sessions.lock().await.insert(session_id, Arc::clone(&session));  // 使用 .await 获取锁

            let session_clone = Arc::clone(&session);

            tokio::spawn(async move {
                loop {
                    // 在锁定期间获取数据包，并在锁定外处理
                    let packet_option = {
                        let mut session_guard = session_clone.lock().await;  // 使用 .await 获取锁
                        session_guard.receive_packet().await
                    };

                    if let Some(packet) = packet_option {
                        // 释放锁后在异步操作中处理数据
                        let session_clone = Arc::clone(&session_clone);
                        tokio::spawn(async move {
                            let mut session_guard = session_clone.lock().await;  // 使用 .await 获取锁
                            if let Err(e) = session_guard.process_packet(packet).await {
                                eprintln!("Error processing packet: {:?}", e);
                            }
                        });
                    } else {
                        break;
                    }
                }
            });
        }
    }
}