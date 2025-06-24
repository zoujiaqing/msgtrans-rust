/// 🎯 单连接传输抽象 - 每个实例只管理一个socket连接
/// 
/// 这是 Transport 的正确抽象：
/// - 每个 Transport 对应一个 socket 连接
/// - 提供 send() 方法直接向 socket 发送数据
/// - 协议无关的设计
/// - 由 TransportClient(单连接) 和 TransportServer(多连接管理) 使用

use std::{
    sync::{Arc, atomic::{AtomicU32, Ordering}},
    collections::HashMap,
};
use tokio::sync::{Mutex, broadcast, oneshot};
use dashmap::DashMap;
use crate::{
    SessionId, TransportError, Packet,
    transport::{
        config::TransportConfig,
        pool::ConnectionPool,
        memory_pool_v2::OptimizedMemoryPool,
        connection_state::ConnectionStateManager,
    },
    protocol::{ProtocolRegistry, ProtocolAdapter},
    connection::Connection,
    adapters::create_standard_registry,
    event::{TransportEvent, RequestContext},
};

/// 🎯 单连接传输抽象 - 真正符合架构设计的 Transport
pub struct Transport {
    /// 配置
    config: TransportConfig,
    /// 协议注册表
    protocol_registry: Arc<ProtocolRegistry>,
    /// 🚀 Phase 3: 优化后的连接池
    connection_pool: Arc<ConnectionPool>,
    /// 🚀 Phase 3: 优化后的内存池
    memory_pool: Arc<OptimizedMemoryPool>,
    /// 🎯 单个连接适配器 - 代表这个socket连接
    connection_adapter: Arc<Mutex<Option<Arc<Mutex<dyn Connection>>>>>,
    /// 当前连接的会话ID
    session_id: Arc<Mutex<Option<SessionId>>>,
    /// 连接状态管理器
    state_manager: ConnectionStateManager,
    event_sender: broadcast::Sender<TransportEvent>,
    request_tracker: Arc<RequestTracker>,
}

pub struct RequestTracker {
    pending: DashMap<u32, oneshot::Sender<Packet>>,
    next_id: AtomicU32,
}

impl RequestTracker {
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
            next_id: AtomicU32::new(1),
        }
    }
    pub fn register(&self) -> (u32, oneshot::Receiver<Packet>) {
        let (tx, rx) = oneshot::channel();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.pending.insert(id, tx);
        (id, rx)
    }
    pub fn complete(&self, id: u32, packet: Packet) -> bool {
        if let Some((_, tx)) = self.pending.remove(&id) {
            let _ = tx.send(packet);
            true
        } else {
            false
        }
    }
    pub fn clear(&self) {
        self.pending.clear();
    }
}



impl Transport {
    /// 创建新的单连接传输
    pub async fn new(config: TransportConfig) -> Result<Self, TransportError> {
        tracing::info!("🚀 创建 Transport");
        
        // 创建协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        // 创建连接池和内存池 (简化版本)
        let connection_pool = Arc::new(
            ConnectionPool::new(2, 10).initialize_pool().await?
        );
        
        let memory_pool = Arc::new(OptimizedMemoryPool::new());
        
        let (event_sender, _) = broadcast::channel(1024);
        
        Ok(Self {
            config,
            protocol_registry,
            connection_pool,
            memory_pool,
            connection_adapter: Arc::new(Mutex::new(None)),
            session_id: Arc::new(Mutex::new(None)),
            state_manager: ConnectionStateManager::new(),
            event_sender,
            request_tracker: Arc::new(RequestTracker::new()),
        })
    }
    
    /// 🎯 核心方法：使用协议配置建立连接
    /// 这是 TransportClient 需要的连接方法
    pub async fn connect_with_config<T>(self: &Arc<Self>, config: T) -> Result<SessionId, TransportError>
    where
        T: crate::protocol::client_config::ConnectableConfig,
    {
        // 直接使用当前的 Transport Arc 实例
        config.connect(Arc::clone(self)).await
    }

    
    /// 🎯 核心方法：发送数据包到当前连接
    pub async fn send(&self, packet: Packet) -> Result<(), TransportError> {
        if let Some(session_id) = self.session_id.lock().await.as_ref() {
            // 🔧 实现真实的发送逻辑
            if let Some(connection_adapter) = &self.connection_adapter.lock().await.as_ref() {
                tracing::debug!("📤 Transport 发送数据包 (会话: {})", session_id);
                
                // 获取锁并直接调用通用的 send 方法
                let mut connection = connection_adapter.lock().await;
                
                tracing::debug!("📤 使用通用连接发送数据包");
                
                // 调用通用的 Connection::send 方法
                match connection.send(packet).await {
                    Ok(_) => {
                        tracing::debug!("📤 数据包发送成功");
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!("📤 数据包发送失败: {:?}", e);
                        Err(e)
                    }
                }
            } else {
                tracing::error!("❌ 没有可用的连接适配器");
                Err(TransportError::connection_error("No connection adapter available", false))
            }
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🎯 核心方法：断开连接（优雅关闭）
    pub async fn disconnect(&self) -> Result<(), TransportError> {
        if let Some(session_id) = self.current_session_id().await {
            self.close_session(session_id).await
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🎯 统一关闭方法：优雅关闭会话
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        // 1. 检查是否可以开始关闭
        if !self.state_manager.try_start_closing(session_id).await {
            tracing::debug!("会话 {} 已经在关闭或已关闭，跳过关闭逻辑", session_id);
            return Ok(());
        }
        
        tracing::info!("🔌 开始优雅关闭会话: {}", session_id);
        
        // 2. 执行实际关闭逻辑（底层适配器会自动发送关闭事件）
        self.do_close_session(session_id).await?;
        
        // 3. 标记为已关闭
        self.state_manager.mark_closed(session_id).await;
        
        // 4. 清理本地状态
        if self.session_id.lock().await.as_ref() == Some(&session_id) {
            *self.session_id.lock().await = None;
            *self.connection_adapter.lock().await = None;
        }
        
        tracing::info!("✅ 会话 {} 关闭完成", session_id);
        Ok(())
    }
    
    /// 🎯 强制关闭会话
    pub async fn force_close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        // 1. 检查是否可以开始关闭
        if !self.state_manager.try_start_closing(session_id).await {
            tracing::debug!("会话 {} 已经在关闭或已关闭，跳过强制关闭", session_id);
            return Ok(());
        }
        
        tracing::info!("🔌 强制关闭会话: {}", session_id);
        
        // 2. 立即强制关闭，不等待
        if let Some(connection_adapter) = &self.connection_adapter.lock().await.as_ref() {
            let mut conn = connection_adapter.lock().await;
            let _ = conn.close().await; // 忽略错误，直接关闭
        }
        
        // 3. 标记为已关闭
        self.state_manager.mark_closed(session_id).await;
        
        // 4. 清理本地状态
        if self.session_id.lock().await.as_ref() == Some(&session_id) {
            *self.session_id.lock().await = None;
            *self.connection_adapter.lock().await = None;
        }
        
        tracing::info!("✅ 会话 {} 强制关闭完成", session_id);
        Ok(())
    }
    
    /// 内部方法：执行实际关闭逻辑
    async fn do_close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        if let Some(connection_adapter) = &self.connection_adapter.lock().await.as_ref() {
            let mut conn = connection_adapter.lock().await;
            
            // 尝试优雅关闭
            match tokio::time::timeout(
                self.config.graceful_timeout,
                self.try_graceful_close(&mut *conn)
            ).await {
                Ok(Ok(_)) => {
                    tracing::debug!("✅ 会话 {} 优雅关闭成功", session_id);
                }
                Ok(Err(e)) => {
                    tracing::warn!("⚠️ 会话 {} 优雅关闭失败，执行强制关闭: {:?}", session_id, e);
                    let _ = conn.close().await; // 忽略错误，直接关闭
                }
                Err(_) => {
                    tracing::warn!("⚠️ 会话 {} 优雅关闭超时，执行强制关闭", session_id);
                    let _ = conn.close().await; // 忽略错误，直接关闭
                }
            }
        }
        
        Ok(())
    }
    
    /// 尝试优雅关闭连接
    async fn try_graceful_close(&self, conn: &mut dyn Connection) -> Result<(), TransportError> {
        // 直接使用底层协议的关闭机制
        // 每个协议都有自己的关闭信号：
        // - QUIC: CONNECTION_CLOSE 帧
        // - TCP: FIN 包  
        // - WebSocket: Close 帧
        tracing::debug!("🔌 使用底层协议的优雅关闭机制");
        conn.close().await
    }
    
    /// 检查连接是否应该忽略消息
    pub async fn should_ignore_messages(&self, session_id: SessionId) -> bool {
        self.state_manager.should_ignore_messages(session_id).await
    }
    
    /// 🎯 核心方法：检查连接状态
    pub async fn is_connected(&self) -> bool {
        self.session_id.lock().await.is_some()
    }
    
    /// 🎯 核心方法：获取当前会话ID
    pub async fn current_session_id(&self) -> Option<SessionId> {
        self.session_id.lock().await.as_ref().cloned()
    }
    
    /// 设置连接适配器和会话ID (内部使用)
    pub async fn set_connection<C>(self: &Arc<Self>, mut connection: C, session_id: SessionId)
    where
        C: Connection + 'static,
    {
        // 🔧 修复：设置连接的 session_id
        connection.set_session_id(session_id);
        
        *self.connection_adapter.lock().await = Some(Arc::new(Mutex::new(connection)));
        *self.session_id.lock().await = Some(session_id);
        self.state_manager.add_connection(session_id);
        tracing::debug!("✅ Transport 连接设置完成: {}", session_id);
        
        // ⭐️ 启动事件消费循环，确保所有 TransportEvent 都在 on_event 里统一处理
        let this = Arc::clone(self);
        let adapter = self.connection_adapter.lock().await.as_ref().unwrap().clone();
        tokio::spawn(async move {
            let conn = adapter.lock().await;
            if let Some(mut event_receiver) = conn.event_stream() {
                drop(conn);
                tracing::debug!("🎧 Transport 事件消费循环启动 (会话: {})", session_id);
                while let Ok(event) = event_receiver.recv().await {
                    tracing::trace!("📥 Transport 收到事件: {:?}", event);
                    this.on_event(event).await;
                }
                tracing::debug!("📡 Transport 事件消费循环结束 (会话: {})", session_id);
            } else {
                tracing::warn!("⚠️ 连接不支持事件流 (会话: {})", session_id);
            }
        });
    }
    
    /// 获取协议注册表
    pub fn protocol_registry(&self) -> &ProtocolRegistry {
        &self.protocol_registry
    }
    
    /// 获取配置
    pub fn config(&self) -> &TransportConfig {
        &self.config
    }
    
    /// 🚀 Phase 3: 获取连接池统计
    pub fn connection_pool_stats(&self) -> crate::transport::pool::OptimizedPoolStatsSnapshot {
        self.connection_pool.get_performance_stats()
    }
    
    /// 🚀 Phase 3: 获取内存池统计
    pub fn memory_pool_stats(&self) -> crate::transport::memory_pool_v2::OptimizedMemoryStatsSnapshot {
        self.memory_pool.get_stats()
    }
    
    /// 获取连接适配器（用于消息接收）
    pub async fn connection_adapter(&self) -> Option<Arc<Mutex<dyn Connection>>> {
        self.connection_adapter.lock().await.as_ref().cloned()
    }
    
    /// 获取连接的事件流（如果支持）
    /// 
    /// 这个方法尝试将连接转换为支持事件流的类型
    pub async fn get_event_stream(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>> {
        if let Some(connection_adapter) = &self.connection_adapter.lock().await.as_ref() {
            let conn = connection_adapter.lock().await;
            
            // 直接调用Connection的get_event_stream方法
            return conn.event_stream();
        }
        
        None
    }

    /// 🚀 发送数据包并等待响应
    pub async fn request(&self, mut packet: Packet) -> Result<Packet, TransportError> {
        if packet.header.packet_type != crate::packet::PacketType::Request {
            return Err(TransportError::connection_error("Not a Request packet", false));
        }
        let (id, rx) = self.request_tracker.register();
        packet.header.message_id = id;
        self.send(packet).await?;
        match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => Err(TransportError::connection_error("Connection closed", false)),
            Err(_) => Err(TransportError::connection_error("Request timeout", false)),
        }
    }

    /// 统一事件处理入口，所有 TransportEvent 都交给这里
    pub async fn on_event(&self, event: crate::event::TransportEvent) {
        match event {
            crate::event::TransportEvent::MessageReceived(packet) => {
                match packet.header.packet_type {
                    crate::packet::PacketType::Response => {
                        let id = packet.header.message_id;
                        tracing::debug!("📥 处理响应包: ID={}, type={:?}", id, packet.header.packet_type);
                        let completed = self.request_tracker.complete(id, packet);
                        tracing::debug!("🔄 响应包处理结果: ID={}, completed={}", id, completed);
                    }
                    crate::packet::PacketType::Request => {
                        let transport = self.clone();
                        let session_id = *self.session_id.lock().await.as_ref().expect("session_id 必须存在");
                        let ctx = crate::event::RequestContext::new(
                            session_id,
                            packet.clone(),
                            Box::new(move |resp| {
                                let transport = transport.clone();
                                tokio::spawn(async move {
                                    let _ = transport.send(resp).await;
                                });
                            }),
                        );
                        let _ = self.event_sender.send(crate::event::TransportEvent::RequestReceived(Arc::new(ctx)));
                    }
                    crate::packet::PacketType::OneWay => {
                        let _ = self.event_sender.send(crate::event::TransportEvent::MessageReceived(packet));
                    }
                }
            }
            // 其它事件直接转发
            _ => {
                let _ = self.event_sender.send(event);
            }
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_sender.subscribe()
    }
}

impl Clone for Transport {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            protocol_registry: self.protocol_registry.clone(),
            connection_pool: self.connection_pool.clone(),
            memory_pool: self.memory_pool.clone(),
            connection_adapter: Arc::new(Mutex::new(None)),
            session_id: Arc::new(Mutex::new(None)),
            state_manager: self.state_manager.clone(),
            event_sender: self.event_sender.clone(),
            request_tracker: self.request_tracker.clone(),
        }
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("connected", &"<async>")
            .field("session_id", &"<async>")
            .finish()
    }
}

