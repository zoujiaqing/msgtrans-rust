/// 🎯 单连接传输抽象 - 每个实例只管理一个socket连接
/// 
/// 这是 Transport 的正确抽象：
/// - 每个 Transport 对应一个 socket 连接
/// - 提供 send() 方法直接向 socket 发送数据
/// - 协议无关的设计
/// - 由 TransportClient(单连接) 和 TransportServer(多连接管理) 使用

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use crate::{
    SessionId, TransportError, Packet,
    transport::{
        config::TransportConfig,
        pool::ConnectionPool,
        memory_pool_v2::OptimizedMemoryPool,
        connection_state::ConnectionStateManager,
        request_manager::RequestManager,
    },

    protocol::{ProtocolRegistry, ProtocolAdapter},
    connection::Connection,
    adapters::create_standard_registry,
    packet::PacketType,
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
    connection_adapter: Option<Arc<Mutex<dyn Connection>>>,
    /// 当前连接的会话ID
    session_id: Option<SessionId>,
    /// 连接状态管理器
    state_manager: ConnectionStateManager,
    /// 🎯 请求响应管理器
    request_manager: Arc<RequestManager>,
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
        
        Ok(Self {
            config,
            protocol_registry,
            connection_pool,
            memory_pool,
            connection_adapter: None,
            session_id: None,
            state_manager: ConnectionStateManager::new(),
            request_manager: Arc::new(RequestManager::new()),
        })
    }
    
    /// 🎯 核心方法：使用协议配置建立连接
    /// 这是 TransportClient 需要的连接方法
    pub async fn connect_with_config<T>(&mut self, config: &T) -> Result<SessionId, TransportError>
    where
        T: super::client::ConnectableConfig,
    {
        // 使用 ConnectableConfig trait 进行实际连接
        match config.connect(self).await {
            Ok(session_id) => {
                self.session_id = Some(session_id);
                
                // 🔧 注意：这里暂时跳过连接适配器的创建
                // 因为真正的协议无关架构应该通过其他方式处理这个问题
                // 例如在 TransportClient 层面管理连接适配器
                tracing::info!("✅ Transport 连接建立成功: {}", session_id);
                Ok(session_id)
            }
            Err(e) => {
                tracing::error!("❌ Transport 连接失败: {:?}", e);
                Err(e)
            }
        }
    }

    
    /// 🎯 核心方法：发送数据包到当前连接
    pub async fn send(&self, packet: Packet) -> Result<(), TransportError> {
        if let Some(session_id) = self.session_id {
            // 🔧 实现真实的发送逻辑
            if let Some(connection_adapter) = &self.connection_adapter {
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

    /// 🎯 核心方法：发送请求并等待响应（默认10秒超时）
    pub async fn request(&self, packet: Packet) -> Result<Packet, TransportError> {
        self.request_with_timeout(packet, Duration::from_secs(10)).await
    }

    /// 🎯 核心方法：发送请求并等待响应（自定义超时）
    pub async fn request_with_timeout(&self, mut packet: Packet, timeout_duration: Duration) -> Result<Packet, TransportError> {
        // 1. 验证包类型
        if packet.packet_type() != PacketType::Request {
            return Err(TransportError::connection_error(
                &format!("Expected Request packet, got {:?}", packet.packet_type()),
                false
            ));
        }

        // 2. 注册请求
        let (message_id, rx) = self.request_manager.register();
        packet.set_message_id(message_id);

        // 3. 发送请求
        self.send(packet).await?;

        // 4. 等待响应（内部消息循环会自动处理 Response 包）
        match timeout(timeout_duration, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(TransportError::connection_error("Connection closed while waiting for response", false)),
            Err(_) => Err(TransportError::connection_error(
                &format!("Request timeout after {:?}", timeout_duration),
                false
            )),
        }
    }

    /// 🎯 已弃用：处理接收到的数据包（现在由内部消息循环处理）
    /// 
    /// 这个方法已被内部消息处理循环替代，不再需要外部调用
    #[deprecated(note = "消息处理现在由 Transport 内部循环自动完成")]
    pub fn handle_incoming_packet(&self, packet: Packet) -> Option<Packet> {
        tracing::warn!("⚠️ handle_incoming_packet 已弃用，消息处理由内部循环自动完成");
        Some(packet) // 直接返回，不处理
    }
    
    /// 🎯 核心方法：断开连接（优雅关闭）
    pub async fn disconnect(&mut self) -> Result<(), TransportError> {
        if let Some(session_id) = self.session_id {
            self.close_session(session_id).await
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🎯 统一关闭方法：优雅关闭会话
    pub async fn close_session(&mut self, session_id: SessionId) -> Result<(), TransportError> {
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
        if self.session_id == Some(session_id) {
            self.session_id = None;
            self.connection_adapter = None;
            // 清理所有 pending requests
            self.request_manager.clear();
        }
        
        tracing::info!("✅ 会话 {} 关闭完成", session_id);
        Ok(())
    }
    
    /// 🎯 强制关闭会话
    pub async fn force_close_session(&mut self, session_id: SessionId) -> Result<(), TransportError> {
        // 1. 检查是否可以开始关闭
        if !self.state_manager.try_start_closing(session_id).await {
            tracing::debug!("会话 {} 已经在关闭或已关闭，跳过强制关闭", session_id);
            return Ok(());
        }
        
        tracing::info!("🔌 强制关闭会话: {}", session_id);
        
        // 2. 立即强制关闭，不等待
        if let Some(connection_adapter) = &self.connection_adapter {
            let mut conn = connection_adapter.lock().await;
            let _ = conn.close().await; // 忽略错误，直接关闭
        }
        
        // 3. 标记为已关闭
        self.state_manager.mark_closed(session_id).await;
        
        // 4. 清理本地状态
        if self.session_id == Some(session_id) {
            self.session_id = None;
            self.connection_adapter = None;
            // 清理所有 pending requests
            self.request_manager.clear();
        }
        
        tracing::info!("✅ 会话 {} 强制关闭完成", session_id);
        Ok(())
    }
    
    /// 内部方法：执行实际关闭逻辑
    async fn do_close_session(&mut self, session_id: SessionId) -> Result<(), TransportError> {
        if let Some(connection_adapter) = &self.connection_adapter {
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
    pub fn is_connected(&self) -> bool {
        self.session_id.is_some()
    }
    
    /// 🎯 核心方法：获取当前会话ID
    pub fn current_session_id(&self) -> Option<SessionId> {
        self.session_id
    }
    
    /// 设置连接适配器和会话ID (内部使用)
    pub fn set_connection<C>(&mut self, connection: C, session_id: SessionId) 
    where
        C: Connection + 'static,
    {
        let connection_adapter = Arc::new(Mutex::new(connection));
        self.connection_adapter = Some(connection_adapter.clone());
        self.session_id = Some(session_id);
        
        // 添加连接状态管理
        self.state_manager.add_connection(session_id);
        
        // 🎯 启动内部消息处理循环
        self.start_internal_message_loop(connection_adapter, session_id);
        
        tracing::debug!("✅ Transport 连接设置完成: {}", session_id);
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
    pub fn connection_adapter(&self) -> Option<Arc<Mutex<dyn Connection>>> {
        self.connection_adapter.clone()
    }
    
    /// 获取连接的事件流（如果支持）
    /// 
    /// 这个方法尝试将连接转换为支持事件流的类型
    pub async fn get_event_stream(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>> {
        if let Some(connection_adapter) = &self.connection_adapter {
            let conn = connection_adapter.lock().await;
            
            // 直接调用Connection的get_event_stream方法
            return conn.event_stream();
        }
        
        None
    }

    /// 🎯 启动内部消息处理循环（正确的架构设计）
    /// 
    /// 在 Transport 内部处理所有接收到的消息，根据 PacketType 进行分发：
    /// - Response 包：直接调用 RequestManager::complete 
    /// - Request 包：转发到上层作为 RequestReceived 事件
    /// - OneWay 包：转发到上层作为 MessageReceived 事件
    fn start_internal_message_loop(&self, connection_adapter: Arc<Mutex<dyn Connection>>, session_id: SessionId) {
        let request_manager = self.request_manager.clone();
        
        tokio::spawn(async move {
            let conn = connection_adapter.lock().await;
            if let Some(mut event_receiver) = conn.event_stream() {
                drop(conn); // 释放锁
                
                tracing::debug!("🔄 Transport 内部消息处理循环启动: {}", session_id);
                
                while let Ok(event) = event_receiver.recv().await {
                    if let crate::event::TransportEvent::MessageReceived { packet, session_id: _ } = event {
                        tracing::debug!("📥 Transport 收到消息: message_id={}, packet_type={:?}", 
                            packet.message_id(), packet.packet_type());
                        
                        match packet.packet_type() {
                            crate::packet::PacketType::Response => {
                                // 🎯 Response 包：直接在 Transport 内部处理
                                let message_id = packet.message_id();
                                if request_manager.complete(message_id, packet) {
                                    tracing::debug!("✅ Response 包处理完成: message_id={}", message_id);
                                } else {
                                    tracing::warn!("⚠️ 收到迟到的 Response 包: message_id={}", message_id);
                                }
                            }
                            
                            crate::packet::PacketType::Request => {
                                // 🎯 Request 包：需要向上层发送 RequestReceived 事件
                                // 这里暂时记录，具体的上层事件发送由 TransportServer 处理
                                tracing::debug!("📨 收到 Request 包，等待上层处理: message_id={}", packet.message_id());
                                // TODO: 发送 RequestReceived 事件到上层
                            }
                            
                            crate::packet::PacketType::OneWay => {
                                // 🎯 OneWay 包：向上层发送 MessageReceived 事件
                                tracing::debug!("📨 收到 OneWay 包，转发到上层: message_id={}", packet.message_id());
                                // TODO: 转发 MessageReceived 事件到上层
                            }
                        }
                    }
                }
                
                tracing::debug!("🔄 Transport 内部消息处理循环结束: {}", session_id);
            } else {
                tracing::debug!("🔄 连接不支持事件流，跳过内部消息处理: {}", session_id);
            }
        });
    }
}

impl Clone for Transport {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            protocol_registry: self.protocol_registry.clone(),
            connection_pool: self.connection_pool.clone(),
            memory_pool: self.memory_pool.clone(),
            connection_adapter: None,  // 克隆时不复制连接
            session_id: None,
            state_manager: ConnectionStateManager::new(),
            request_manager: Arc::new(RequestManager::new()),
        }
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("connected", &self.is_connected())
            .field("session_id", &self.session_id)
            .finish()
    }
} 