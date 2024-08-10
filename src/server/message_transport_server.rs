use crate::channel::ServerChannel;
use crate::packet::Packet;
use crate::session::TransportSession;
use crossbeam::channel::{unbounded, Sender};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

pub struct MessageTransportServer {
    channels: Arc<RwLock<Vec<Arc<RwLock<dyn ServerChannel + Send + Sync>>>>>,
    sessions: Arc<RwLock<HashMap<usize, Arc<RwLock<dyn TransportSession + Send + Sync>>>>>,
    next_id: Arc<AtomicUsize>,
    message_sender: Sender<(Packet, Arc<RwLock<dyn TransportSession + Send + Sync>>)>,
}

impl MessageTransportServer {
    pub fn new() -> Self {
        let (message_sender, message_receiver) = unbounded();

        let server = MessageTransportServer {
            channels: Arc::new(RwLock::new(Vec::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicUsize::new(1)),
            message_sender,
        };

        thread::spawn(move || {
            while let Ok((packet, session)) = message_receiver.recv() {
                let session_clone = Arc::clone(&session);

                // 在同步块外获取处理器并提前释放锁
                let handler = {
                    let session_guard = session_clone.read().unwrap();
                    session_guard.get_message_handler() // 获取处理器
                };

                // 处理消息的同步块
                if let Some(handler) = handler {
                    let handler = handler.read().unwrap();
                    handler(packet, session_clone);
                } else {
                    let mut session_guard = session_clone.write().unwrap();
                    session_guard.process_packet(packet);
                }
            }
        });

        server
    }

    pub fn start(self) {
        let channels = Arc::clone(&self.channels);
        let channels_vec = {
            let channels_lock = channels.read().unwrap();
            channels_lock.clone()
        };

        for channel in channels_vec.into_iter() {
            let channel_clone = Arc::clone(&channel);
            let sessions_clone = Arc::clone(&self.sessions);
            let next_id_clone = Arc::clone(&self.next_id);

            // 同步启动通道
            let mut channel_guard = channel_clone.write().unwrap();
            channel_guard.start(sessions_clone, next_id_clone);
        }
    }

    pub fn add_channel(&mut self, channel: Arc<RwLock<dyn ServerChannel + Send + Sync>>) {
        self.channels.write().unwrap().push(channel);
    }

    pub fn set_message_handler(
        &mut self,
        handler: Arc<
            RwLock<
                Box<dyn Fn(Packet, Arc<RwLock<dyn TransportSession + Send + Sync>>) + Send + Sync>,
            >,
        >,
    ) {
        let sessions = Arc::clone(&self.sessions);
        let sessions_lock = sessions.write().unwrap();
        for session in sessions_lock.values() {
            session
                .write()
                .unwrap()
                .set_message_handler(handler.clone());
        }
    }
}