/// 连接状态管理
/// 
/// 用于统一的连接关闭机制，防止重复关闭和竞态条件

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::SessionId;

/// 连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// 已连接，正常工作
    Connected,
    /// 正在关闭中，忽略所有消息
    Closing,
    /// 已关闭
    Closed,
}

/// 连接状态管理器
/// 
/// 提供线程安全的状态管理，防止重复关闭
pub struct ConnectionStateManager {
    states: Arc<crate::transport::lockfree::LockFreeHashMap<SessionId, Arc<Mutex<ConnectionState>>>>,
}

impl ConnectionStateManager {
    /// 创建新的状态管理器
    pub fn new() -> Self {
        Self {
            states: Arc::new(crate::transport::lockfree::LockFreeHashMap::new()),
        }
    }
    
    /// 添加新连接
    pub fn add_connection(&self, session_id: SessionId) {
        let state = Arc::new(Mutex::new(ConnectionState::Connected));
        self.states.insert(session_id, state);
    }
    
    /// 尝试开始关闭连接
    /// 
    /// 返回 true 表示可以开始关闭，false 表示已经在关闭或已关闭
    pub async fn try_start_closing(&self, session_id: SessionId) -> bool {
        if let Some(state_lock) = self.states.get(&session_id) {
            let mut state = state_lock.lock().await;
            match *state {
                ConnectionState::Connected => {
                    *state = ConnectionState::Closing;
                    true
                }
                ConnectionState::Closing | ConnectionState::Closed => false,
            }
        } else {
            false
        }
    }
    
    /// 标记连接为已关闭
    pub async fn mark_closed(&self, session_id: SessionId) {
        if let Some(state_lock) = self.states.get(&session_id) {
            let mut state = state_lock.lock().await;
            *state = ConnectionState::Closed;
        }
    }
    
    /// 检查连接是否应该忽略消息
    pub async fn should_ignore_messages(&self, session_id: SessionId) -> bool {
        if let Some(state_lock) = self.states.get(&session_id) {
            let state = state_lock.lock().await;
            matches!(*state, ConnectionState::Closing | ConnectionState::Closed)
        } else {
            true // 未知连接，忽略消息
        }
    }
    
    /// 获取连接状态
    pub async fn get_state(&self, session_id: SessionId) -> Option<ConnectionState> {
        if let Some(state_lock) = self.states.get(&session_id) {
            let state = state_lock.lock().await;
            Some(state.clone())
        } else {
            None
        }
    }
    
    /// 移除连接状态
    pub fn remove_connection(&self, session_id: SessionId) {
        self.states.remove(&session_id);
    }
    
    /// 获取所有连接状态（用于调试）
    pub async fn get_all_states(&self) -> Vec<(SessionId, ConnectionState)> {
        let mut result = Vec::new();
        self.states.for_each(|session_id, state_lock| {
            // 注意：这里不能使用 await，所以只能返回当前可获取的状态
            if let Ok(state) = state_lock.try_lock() {
                result.push((*session_id, state.clone()));
            }
        });
        result
    }
}

impl Default for ConnectionStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ConnectionStateManager {
    fn clone(&self) -> Self {
        Self {
            states: self.states.clone(),
        }
    }
}

impl std::fmt::Debug for ConnectionStateManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionStateManager")
            .field("states_count", &self.states.len())
            .finish()
    }
} 