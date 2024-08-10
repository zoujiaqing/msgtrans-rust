use crate::channel::ServerChannel;
use crate::packet::Packet;
use crate::session::TransportSession;
use crossbeam::channel::{unbounded, Sender};
use std::collections::HashMap;
use std::sync::{Arc};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex; // 使用 tokio::sync::Mutex
use tokio::task;

pub struct MessageTransportServer {
    channels: Arc<Mutex<Vec<Arc<Mutex<dyn ServerChannel + Send + Sync>>>>>,  // 使用 tokio::sync::Mutex
    sessions: Arc<Mutex<HashMap<usize, Arc<Mutex<dyn TransportSession + Send + Sync>>>>>,  // 使用 tokio::sync::Mutex
    next_id: Arc<AtomicUsize>,
    message_sender: Sender<(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>)>,
}

impl MessageTransportServer {
    pub fn new() -> Self {
        let (message_sender, message_receiver) = unbounded();

        let server = MessageTransportServer {
            channels: Arc::new(Mutex::new(Vec::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicUsize::new(1)),
            message_sender,
        };

        // 使用异步任务来处理消息
        task::spawn(async move {
            while let Ok((packet, session)) = message_receiver.recv() {
                let session_clone = Arc::clone(&session);

                // 在异步块外获取处理器并提前释放锁
                let handler = {
                    let session_guard = session_clone.lock().await;
                    session_guard.get_message_handler() // 获取处理器
                };

                // 处理消息的异步块
                if let Some(handler) = handler {
                    let handler = handler.lock().await;
                    handler(packet, session_clone);
                } else {
                    let mut session_guard = session_clone.lock().await;
                    session_guard.process_packet(packet).await;
                }
            }
        });

        server
    }

    pub async fn start(self) {
        let channels = Arc::clone(&self.channels);
        let channels_vec = {
            let channels_lock = channels.lock().await;
            channels_lock.clone()
        };

        for channel in channels_vec.into_iter() {
            let channel_clone = Arc::clone(&channel);
            let sessions_clone = Arc::clone(&self.sessions);
            let next_id_clone = Arc::clone(&self.next_id);

            // 异步启动通道
            let mut channel_guard = channel_clone.lock().await;
            channel_guard.start(sessions_clone, next_id_clone).await;
        }
    }

    pub fn add_channel(&mut self, channel: Arc<Mutex<dyn ServerChannel + Send + Sync>>) {
        let mut channels_lock = self.channels.blocking_lock();  // 在同步上下文中使用 blocking_lock
        channels_lock.push(channel);
    }

    pub fn set_message_handler(
        &mut self,
        handler: Arc<
            Mutex<
                Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>,
            >,
        >,
    ) {
        let sessions = Arc::clone(&self.sessions);
        let mut sessions_lock = sessions.blocking_lock();  // 在同步上下文中使用 blocking_lock
        for session in sessions_lock.values() {
            let mut session_guard = session.blocking_lock();  // 在同步上下文中使用 blocking_lock
            session_guard.set_message_handler(handler.clone());
        }
    }
}