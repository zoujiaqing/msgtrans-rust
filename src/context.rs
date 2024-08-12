use std::sync::Arc;
use crate::session::TransportSession;

pub struct Context {
    session: Arc<dyn TransportSession + Send + Sync>,
}

impl Context {
    // 创建新的 Context 实例
    pub fn new(session: Arc<dyn TransportSession + Send + Sync>) -> Self {
        Context { session }
    }

    // 获取 session 的 Arc 引用
    pub fn session(&self) -> Arc<dyn TransportSession + Send + Sync> {
        Arc::clone(&self.session)
    }
}