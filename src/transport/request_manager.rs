use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::oneshot;
use crate::transport::lockfree_enhanced::LockFreeHashMap;
use crate::packet::Packet;

/// 包装 oneshot::Sender 使其可 Clone
#[derive(Clone)]
struct SenderWrapper {
    inner: Arc<std::sync::Mutex<Option<oneshot::Sender<Packet>>>>,
}

impl SenderWrapper {
    fn new(sender: oneshot::Sender<Packet>) -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(Some(sender))),
        }
    }
    
    fn send(self, packet: Packet) -> Result<(), Packet> {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(sender) = guard.take() {
                return sender.send(packet);
            }
        }
        Err(packet)
    }
}

/// 无锁请求管理器
pub struct RequestManager {
    pending: LockFreeHashMap<u32, SenderWrapper>,
    message_id_counter: AtomicU32,
}

impl RequestManager {
    pub fn new() -> Self {
        Self {
            pending: LockFreeHashMap::with_capacity(64),
            message_id_counter: AtomicU32::new(1),
        }
    }
    
    /// 注册请求，返回 message_id 和接收器
    pub fn register(&self) -> (u32, oneshot::Receiver<Packet>) {
        let (tx, rx) = oneshot::channel();
        let message_id = self.message_id_counter.fetch_add(1, Ordering::Relaxed);
        
        // 如果插入失败，直接 panic（底层库，不应该发生）
        self.pending.insert(message_id, SenderWrapper::new(tx)).expect("Failed to register request");
        
        (message_id, rx)
    }
    
    /// 完成请求
    pub fn complete(&self, message_id: u32, packet: Packet) -> bool {
        match self.pending.remove(&message_id) {
            Ok(Some(sender)) => {
                let _ = sender.send(packet);
                true
            }
            _ => false,
        }
    }
    
    /// 清理所有待处理请求（连接关闭时）
    pub fn clear(&self) {
        if let Ok(snapshot) = self.pending.snapshot() {
            for (message_id, _) in snapshot {
                let _ = self.pending.remove(&message_id);
            }
        }
    }
} 