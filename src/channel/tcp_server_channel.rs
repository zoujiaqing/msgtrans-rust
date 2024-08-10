use super::ServerChannel;
use crate::session::{TransportSession, TcpTransportSession};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;

pub struct TcpServerChannel {
    port: u16,
}

impl TcpServerChannel {
    pub fn new(port: u16) -> Self {
        TcpServerChannel { port }
    }
}

#[async_trait::async_trait]
impl ServerChannel for TcpServerChannel {
    async fn start(
        &mut self,
        sessions: Arc<RwLock<HashMap<usize, Arc<RwLock<dyn TransportSession + Send + Sync>>>>>,
        next_id: Arc<AtomicUsize>,
    ) {
        let listener = TcpListener::bind(("0.0.0.0", self.port)).await.unwrap();
        while let Ok((stream, _)) = listener.accept().await {
            let session_id = next_id.fetch_add(1, Ordering::SeqCst);

            let session: Arc<RwLock<dyn TransportSession + Send + Sync>> =
                Arc::new(RwLock::new(TcpTransportSession::new(stream, session_id)));
            sessions.write().unwrap().insert(session_id, Arc::clone(&session));

            let session_clone = Arc::clone(&session);

            tokio::spawn(async move {
                loop {
                    // 获取包数据，并在锁定外进行处理
                    let packet = {
                        let mut session_guard = session_clone.write().unwrap();
                        session_guard.receive_packet().await
                    };

                    if let Some(packet) = packet {
                        // 在锁定外部执行异步操作
                        let session_clone = Arc::clone(&session_clone);
                        tokio::spawn(async move {
                            let mut session_guard = session_clone.write().unwrap();
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