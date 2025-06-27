/// 服务端传输层实现
/// 
/// 提供多协议服务端支持，管理会话和连接

use std::sync::Arc;
use crate::{
    SessionId, TransportError, Packet, EventStream,
    transport::{
        config::TransportConfig,
        lockfree::LockFreeHashMap,
        connection_state::ConnectionStateManager,
    },
    error::CloseReason,
    command::{ConnectionInfo, TransportStats},
    Connection, Server,
    event::ServerEvent,
    protocol::adapter::DynServerConfig,
};
use tokio::sync::broadcast;

/// TransportServer - 多协议服务端
/// 
/// 🎯 设计目标：
/// - 多协议支持
/// - 高并发连接管理
/// - 统一的事件系统
pub struct TransportServer {
    /// 配置
    config: TransportConfig,
    /// 🎯 核心：会话到传输层的映射 (统一使用 Transport 抽象)
    transports: Arc<LockFreeHashMap<SessionId, Arc<crate::transport::transport::Transport>>>,
    /// 会话ID生成器
    session_id_generator: Arc<std::sync::atomic::AtomicU64>,
    /// 服务端统计信息 (使用 lockfree)
    stats: Arc<LockFreeHashMap<SessionId, TransportStats>>,
    /// 事件广播器
    event_sender: broadcast::Sender<ServerEvent>,
    /// 是否正在运行
    is_running: Arc<std::sync::atomic::AtomicBool>,
    /// 🔧 协议配置 - 改为服务端专用配置
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynServerConfig>>,
    /// 连接状态管理器
    state_manager: ConnectionStateManager,
    /// 🎯 新增：请求跟踪器 - 支持服务端向客户端发送请求
    request_tracker: Arc<crate::transport::transport::RequestTracker>,
    /// 🎯 消息ID计数器 - 用于自动生成消息ID
    message_id_counter: std::sync::atomic::AtomicU32,
}

impl TransportServer {
    /// 创建新的 TransportServer
    pub async fn new(config: TransportConfig) -> Result<Self, TransportError> {
        let (event_sender, _) = broadcast::channel(1000);
        
        Ok(Self {
            config,
            transports: Arc::new(LockFreeHashMap::new()),
            session_id_generator: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            stats: Arc::new(LockFreeHashMap::new()),
            event_sender,
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            protocol_configs: std::collections::HashMap::new(),
            state_manager: ConnectionStateManager::new(),
            request_tracker: Arc::new(crate::transport::transport::RequestTracker::new_with_start_id(10000)),
            message_id_counter: std::sync::atomic::AtomicU32::new(20000), // 服务端使用更高的ID范围
        })
    }

    /// 🔧 内部方法：创建带协议配置的服务端（由 TransportServerBuilder 调用）
    pub async fn new_with_protocols(
        config: TransportConfig,
        protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynServerConfig>>
    ) -> Result<Self, TransportError> {
        let (event_sender, _) = broadcast::channel(1000);
        
        Ok(Self {
            config,
            transports: Arc::new(LockFreeHashMap::new()),
            session_id_generator: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            stats: Arc::new(LockFreeHashMap::new()),
            event_sender,
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            protocol_configs,
            state_manager: ConnectionStateManager::new(),
            request_tracker: Arc::new(crate::transport::transport::RequestTracker::new_with_start_id(10000)),
            message_id_counter: std::sync::atomic::AtomicU32::new(20000), // 服务端使用更高的ID范围
        })
    }

    /// 🚀 向指定会话发送数据包 - 无锁版本
    pub async fn send_to_session(&self, session_id: SessionId, packet: Packet) -> Result<(), TransportError> {
        tracing::debug!("📤 TransportServer 向会话 {} 发送数据包 (ID: {}, 大小: {} bytes)", 
            session_id, packet.header.message_id, packet.payload.len());
        
        if let Some(transport) = self.transports.get(&session_id) {
            // 🚀 状态检查 - 通过 Transport 抽象
            if !transport.is_connected().await {
                tracing::warn!("⚠️ 会话 {} 连接已断开，跳过发送", session_id);
                // 清理已断开的连接
                let _ = self.remove_session(session_id).await;
                return Err(TransportError::connection_error("Connection closed", false));
            }
            
            tracing::debug!("🔍 会话 {} 连接状态正常，开始发送数据包", session_id);
            
            // 🚀 通过 Transport 统一接口发送
            match transport.send(packet).await {
                Ok(()) => {
                    tracing::debug!("✅ 会话 {} 发送成功 (TransportServer层确认)", session_id);
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("❌ 会话 {} 发送失败: {:?}", session_id, e);
                    
                    // 🔧 关键修复：检查是否是连接相关错误
                    let error_msg = format!("{:?}", e);
                    if error_msg.contains("Broken pipe") || 
                       error_msg.contains("Connection reset") || 
                       error_msg.contains("Connection closed") ||
                       error_msg.contains("ECONNRESET") ||
                       error_msg.contains("EPIPE") {
                        tracing::warn!("⚠️ 会话 {} 连接已断开: {}", session_id, error_msg);
                        // 清理已断开的连接
                        let _ = self.remove_session(session_id).await;
                        return Err(TransportError::connection_error("Connection closed during send", false));
                    } else {
                        tracing::error!("❌ 会话 {} 发送失败 (非连接错误): {:?}", session_id, e);
                        return Err(e);
                    }
                }
            }
        } else {
            tracing::warn!("⚠️ 会话 {} 不存在于连接映射中", session_id);
            Err(TransportError::connection_error("Session not found", false))
        }
    }

    /// 🚀 向指定会话发送请求并等待响应 - 使用 Transport 的 request 方法
    pub async fn request_to_session(&self, session_id: SessionId, packet: Packet) -> Result<Packet, TransportError> {
        tracing::debug!("🔄 TransportServer 向会话 {} 发送请求 (ID: {})", session_id, packet.header.message_id);
        
        if let Some(transport) = self.transports.get(&session_id) {
            // 🚀 状态检查 - 通过 Transport 抽象
            if !transport.is_connected().await {
                tracing::warn!("⚠️ 会话 {} 连接已断开，无法发送请求", session_id);
                let _ = self.remove_session(session_id).await;
                return Err(TransportError::connection_error("Connection closed", false));
            }
            
            // 🎯 直接使用 Transport 的 request 方法，它会正确管理 RequestTracker
            match transport.request(packet).await {
                Ok(response) => {
                    tracing::debug!("✅ 会话 {} 收到响应 (响应ID: {})", session_id, response.header.message_id);
                    Ok(response)
                }
                Err(e) => {
                    tracing::error!("❌ 会话 {} 请求失败: {:?}", session_id, e);
                    Err(e)
                }
            }
        } else {
            tracing::warn!("⚠️ 会话 {} 不存在于连接映射中", session_id);
            Err(TransportError::connection_error("Session not found", false))
        }
    }

    /// 🚀 向指定会话发送字节数据 - 统一API返回TransportResult
    pub async fn send(&self, session_id: SessionId, data: &[u8]) -> Result<crate::event::TransportResult, TransportError> {
        let message_id = self.message_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let packet = crate::packet::Packet::one_way(message_id, data.to_vec());
        
        tracing::debug!("TransportServer 向会话 {} 发送数据: {} bytes (ID: {})", session_id, data.len(), message_id);
        
        match self.send_to_session(session_id, packet).await {
            Ok(()) => {
                // 发送成功，返回TransportResult
                Ok(crate::event::TransportResult::new_sent(Some(session_id), message_id))
            }
            Err(e) => Err(e),
        }
    }
    
    /// 🔄 向指定会话发送字节请求并等待响应 - 统一API返回TransportResult
    pub async fn request(&self, session_id: SessionId, data: &[u8]) -> Result<crate::event::TransportResult, TransportError> {
        let message_id = self.message_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let packet = crate::packet::Packet::request(message_id, data.to_vec());
        
        tracing::debug!("TransportServer 向会话 {} 发送请求: {} bytes (ID: {})", session_id, data.len(), message_id);
        
        match self.request_to_session(session_id, packet).await {
            Ok(response_packet) => {
                tracing::debug!("TransportServer 收到会话 {} 的响应: {} bytes (ID: {})", session_id, response_packet.payload.len(), response_packet.header.message_id);
                // 请求成功，返回包含响应数据的TransportResult
                Ok(crate::event::TransportResult::new_completed(Some(session_id), message_id, response_packet.payload.clone()))
            }
            Err(e) => {
                // 判断是否为超时错误
                if e.to_string().contains("timeout") || e.to_string().contains("Timeout") {
                    Ok(crate::event::TransportResult::new_timeout(Some(session_id), message_id))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// 🚀 添加会话 - 使用 Transport 抽象
    pub async fn add_session(&self, connection: Box<dyn crate::Connection>) -> SessionId {
        // 🔧 修复：使用连接已有的会话ID，而不是生成新的
        let session_id = connection.session_id();
        
        // 🚀 创建 Transport 实例
        let transport = Arc::new(crate::transport::transport::Transport::new(self.config.clone()).await.unwrap());
        
        // 设置连接到 Transport
        transport.set_connection(connection, session_id).await;
        
        // 插入到传输层映射中
        self.transports.insert(session_id, transport.clone());
        self.stats.insert(session_id, TransportStats::new());
        
        // 注册连接状态
        self.state_manager.add_connection(session_id);
        
        // ⭐️ 启动事件消费循环，将 TransportEvent 转换为 ServerEvent
        let server_clone = self.clone();
        let transport_for_events = transport.clone();
        tokio::spawn(async move {
            if let Some(mut event_receiver) = transport_for_events.get_event_stream().await {
                tracing::debug!("🎧 TransportServer 启动会话 {} 的事件消费循环", session_id);
                while let Ok(transport_event) = event_receiver.recv().await {
                    tracing::trace!("📥 TransportServer 收到会话 {} 的事件: {:?}", session_id, transport_event);
                    server_clone.handle_transport_event(session_id, transport_event).await;
                }
                tracing::debug!("📡 TransportServer 会话 {} 的事件消费循环结束", session_id);
            } else {
                tracing::warn!("⚠️ 会话 {} 无法获取事件流", session_id);
            }
        });
        
        tracing::info!("✅ TransportServer 添加会话: {} (使用 Transport 抽象)", session_id);
        session_id
    }

    /// 移除会话
    pub async fn remove_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        self.transports.remove(&session_id);
        self.stats.remove(&session_id);
        self.state_manager.remove_connection(session_id);
        tracing::info!("🗑️ TransportServer 移除会话: {}", session_id);
        Ok(())
    }
    
    /// 🎯 统一关闭方法：优雅关闭会话
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        // 1. 检查是否可以开始关闭
        if !self.state_manager.try_start_closing(session_id).await {
            tracing::debug!("会话 {} 已经在关闭或已关闭，跳过关闭逻辑", session_id);
            return Ok(());
        }
        
        tracing::info!("🔌 开始优雅关闭会话: {}", session_id);
        
        // 2. 发送连接关闭事件（在资源清理前）
        let close_event = ServerEvent::ConnectionClosed {
            session_id,
            reason: crate::error::CloseReason::Normal,
        };
        let _ = self.event_sender.send(close_event);
        
        // 3. 执行实际关闭逻辑
        self.do_close_session(session_id).await?;
        
        // 4. 标记为已关闭
        self.state_manager.mark_closed(session_id).await;
        
        // 5. 清理会话
        self.remove_session(session_id).await?;
        
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
        
        // 2. 发送连接关闭事件
        let close_event = ServerEvent::ConnectionClosed {
            session_id,
            reason: crate::error::CloseReason::Forced,
        };
        let _ = self.event_sender.send(close_event);
        
        // 3. 立即强制关闭，不等待
        if let Some(transport) = self.transports.get(&session_id) {
            let _ = transport.disconnect().await; // 忽略错误，直接关闭
        }
        
        // 4. 标记为已关闭
        self.state_manager.mark_closed(session_id).await;
        
        // 5. 清理会话
        self.remove_session(session_id).await?;
        
        tracing::info!("✅ 会话 {} 强制关闭完成", session_id);
        Ok(())
    }
    
    /// 🎯 批量关闭所有会话
    pub async fn close_all_sessions(&self) -> Result<(), TransportError> {
        let session_ids = self.active_sessions().await;
        let total_sessions = session_ids.len();
        
        if total_sessions == 0 {
            tracing::info!("没有活跃会话需要关闭");
            return Ok(());
        }
        
        tracing::info!("🔌 开始批量关闭 {} 个会话", total_sessions);
        
        // 使用 graceful_timeout 作为批量关闭的总超时时间
        let start_time = std::time::Instant::now();
        let timeout = self.config.graceful_timeout;
        
        let mut success_count = 0;
        let mut error_count = 0;
        
        for session_id in session_ids {
            // 检查是否超时
            if start_time.elapsed() >= timeout {
                tracing::warn!("⚠️ 批量关闭超时，剩余会话将被强制关闭");
                // 强制关闭剩余会话
                let _ = self.force_close_session(session_id).await;
                continue;
            }
            
            // 尝试优雅关闭
            match self.close_session(session_id).await {
                Ok(_) => success_count += 1,
                Err(e) => {
                    error_count += 1;
                    tracing::warn!("⚠️ 关闭会话 {} 失败: {:?}", session_id, e);
                }
            }
        }
        
        tracing::info!("✅ 批量关闭完成，成功: {}, 失败: {}", success_count, error_count);
        Ok(())
    }
    
    /// 内部方法：执行实际关闭逻辑 - 通过 Transport 抽象
    async fn do_close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        if let Some(transport) = self.transports.get(&session_id) {
            // 尝试优雅关闭
            match tokio::time::timeout(
                self.config.graceful_timeout,
                transport.disconnect()
            ).await {
                Ok(Ok(_)) => {
                    tracing::debug!("✅ 会话 {} 优雅关闭成功", session_id);
                }
                Ok(Err(e)) => {
                    tracing::warn!("⚠️ 会话 {} 优雅关闭失败: {:?}", session_id, e);
                    // 优雅关闭失败，但不返回错误，继续清理
                }
                Err(_) => {
                    tracing::warn!("⚠️ 会话 {} 优雅关闭超时", session_id);
                    // 超时，但不返回错误，继续清理
                }
            }
        }
        
        Ok(())
    }
    
    /// 检查连接是否应该忽略消息
    pub async fn should_ignore_messages(&self, session_id: SessionId) -> bool {
        self.state_manager.should_ignore_messages(session_id).await
    }

    /// 广播消息到所有会话
    pub async fn broadcast(&self, packet: Packet) -> Result<(), TransportError> {
        let mut success_count = 0;
        let mut error_count = 0;
        
        // 先收集所有会话ID，然后逐个处理
        let session_ids: Vec<SessionId> = self.transports.keys().unwrap_or_default();
        for session_id in session_ids {
            if let Some(transport) = self.transports.get(&session_id) {
                match transport.send(packet.clone()).await {
                    Ok(()) => success_count += 1,
                    Err(e) => {
                        error_count += 1;
                        tracing::warn!("⚠️ 广播到会话 {} 失败: {:?}", session_id, e);
                    }
                }
            }
        }
        
        if error_count > 0 {
            tracing::warn!("⚠️ 广播完成，成功: {}, 失败: {}", success_count, error_count);
        } else {
            tracing::info!("✅ 广播完成，成功: {}", success_count);
        }
        
        Ok(())
    }

    /// 获取活跃会话列表
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        self.transports.keys().unwrap_or_default()
    }

    /// 获取会话计数
    pub async fn session_count(&self) -> usize {
        self.transports.len()
    }

    /// 生成新的会话ID
    fn generate_session_id(&self) -> SessionId {
        let id = self.session_id_generator.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SessionId(id)
    }

    /// 业务层订阅 ServerEvent
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<crate::event::ServerEvent> {
        self.event_sender.subscribe()
    }

    /// 启动服务端
    pub async fn serve(&self) -> Result<(), TransportError> {
        self.is_running.store(true, std::sync::atomic::Ordering::SeqCst);
        
        if self.protocol_configs.is_empty() {
            tracing::warn!("⚠️ 没有配置协议，服务端无法启动监听");
            return Err(TransportError::config_error("protocols", "No protocols configured"));
        }
        
        tracing::info!("🌟 启动 {} 个协议服务器", self.protocol_configs.len());
        
        // 创建监听任务的向量
        let mut listen_tasks = Vec::new();
        
        // 为每个协议配置启动服务器
        for (protocol_name, protocol_config) in &self.protocol_configs {
            tracing::info!("🔧 处理协议: {}", protocol_name);
            
            let address = self.get_protocol_bind_address(protocol_config);
            tracing::info!("📍 协议 {} 的绑定地址: {}", protocol_name, address);
            
            match protocol_config.build_server_dyn().await {
                Ok(server) => {
                    match self.start_protocol_listener(server, protocol_name.clone()).await {
                        Ok(listener_task) => {
                            listen_tasks.push(listener_task);
                            tracing::info!("✅ {} 服务器启动成功: {}", protocol_name, address);
                        }
                        Err(e) => {
                            tracing::error!("❌ {} 监听任务创建失败: {:?}", protocol_name, e);
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("❌ {} 服务器构建失败: {:?}", protocol_name, e);
                    return Err(e);
                }
            }
        }
        
        tracing::info!("🎯 所有协议服务器启动完成，等待连接...");
        
        // 等待所有监听任务完成
        for (index, task) in listen_tasks.into_iter().enumerate() {
            tracing::info!("⏳ 等待第 {} 个监听任务完成...", index + 1);
            if let Err(e) = task.await {
                tracing::error!("❌ 第 {} 个监听任务被取消: {:?}", index + 1, e);
                return Err(TransportError::config_error("server", "Listener task cancelled"));
            }
        }
        
        tracing::info!("🛑 TransportServer 已停止");
        Ok(())
    }

    /// 🎯 启动协议监听器 - 通用方法
    async fn start_protocol_listener(&self, mut server: Box<dyn crate::Server>, protocol_name: String) -> Result<tokio::task::JoinHandle<()>, TransportError>
    {
        let server_clone = self.clone();
        
        let task = tokio::spawn(async move {
            tracing::info!("🚀 {} 监听任务已启动", protocol_name);
            
            let mut accept_count = 0u64;
            
            loop {
                tracing::debug!("🔄 {} 等待连接... (接受计数: {})", protocol_name, accept_count);
                
                match server.accept().await {
                    Ok(mut connection) => {
                        accept_count += 1;
                        tracing::info!("🎉 {} accept 成功! 连接 #{}", protocol_name, accept_count);
                        
                        // 获取连接信息
                        let connection_info = connection.connection_info();
                        let peer_addr = connection_info.peer_addr;
                        
                        tracing::info!("🔗 新的 {} 连接 #{}: {}", protocol_name, accept_count, peer_addr);
                        
                        // 生成新的会话ID并设置到连接
                        let session_id = server_clone.generate_session_id();
                        connection.set_session_id(session_id);
                        tracing::info!("🆔 为 {} 连接生成会话ID: {}", protocol_name, session_id);
                        
                        // 添加到会话管理
                        let actual_session_id = server_clone.add_session(connection).await;
                        
                        // 发送连接建立事件
                        let connect_event = ServerEvent::ConnectionEstablished { 
                            session_id: actual_session_id,
                            info: connection_info,
                        };
                        let _ = server_clone.event_sender.send(connect_event);
                        tracing::info!("📨 {} 连接事件已发送", protocol_name);
                    }
                    Err(e) => {
                        tracing::error!("❌ {} 接受连接失败: {:?}", protocol_name, e);
                        break;
                    }
                }
            }
            
            tracing::info!("🛑 {} 服务器已停止", protocol_name);
        });
        
        Ok(task)
    }

    /// 🔧 内部方法：从协议配置中提取监听地址
    fn get_protocol_bind_address(&self, protocol_config: &Box<dyn crate::protocol::adapter::DynServerConfig>) -> std::net::SocketAddr {
        protocol_config.get_bind_address()
    }

    /// 🎯 处理连接的 TransportEvent，转换为 ServerEvent
    async fn handle_transport_event(&self, session_id: SessionId, transport_event: crate::event::TransportEvent) {
        tracing::debug!("🎯 handle_transport_event: 会话 {}, 事件: {:?}", session_id, transport_event);
        match transport_event {
            crate::event::TransportEvent::MessageReceived(packet) => {
                tracing::debug!("📥 收到MessageReceived事件，数据包类型: {:?}, ID: {}", packet.header.packet_type, packet.header.message_id);
                match packet.header.packet_type {
                    crate::packet::PacketType::Request => {
                        tracing::debug!("🔄 处理Request类型的数据包 (ID: {})", packet.header.message_id);
                        // 创建统一的TransportContext
                        let server_clone = self.clone();
                        let mut context = crate::event::TransportContext::new_request(
                            Some(session_id),
                            packet.header.message_id,
                            packet.header.biz_type,
                            if packet.ext_header.is_empty() { None } else { Some(packet.ext_header.clone()) },
                            packet.payload.clone(),
                            std::sync::Arc::new(move |response_data: Vec<u8>| {
                                let server = server_clone.clone();
                                tokio::spawn(async move {
                                    let response_packet = crate::packet::Packet {
                                        header: crate::packet::FixedHeader {
                                            version: 1,
                                            compression: crate::packet::CompressionType::None,
                                            packet_type: crate::packet::PacketType::Response,
                                            biz_type: 0,
                                            message_id: packet.header.message_id,
                                            ext_header_len: 0,
                                            payload_len: response_data.len() as u32,
                                            reserved: crate::packet::ReservedFlags::new(),
                                        },
                                        ext_header: Vec::new(),
                                        payload: response_data,
                                    };
                                    let _ = server.send_to_session(session_id, response_packet).await;
                                });
                            }),
                        );
                        
                        // 🚀 修复BUG：设置为主实例，确保请求被正确跟踪和响应
                        context.set_primary();
                        tracing::debug!("✅ 设置TransportContext为主实例 (ID: {})", packet.header.message_id);
                        
                        let event = crate::event::ServerEvent::MessageReceived { 
                            session_id, 
                            context 
                        };
                        tracing::debug!("📤 准备发送ServerEvent::MessageReceived (会话: {}, ID: {})", session_id, packet.header.message_id);
                        
                        match self.event_sender.send(event) {
                            Ok(receivers) => {
                                tracing::debug!("✅ ServerEvent发送成功，接收者数量: {} (会话: {}, ID: {})", receivers, session_id, packet.header.message_id);
                            }
                            Err(e) => {
                                tracing::error!("❌ ServerEvent发送失败: {:?} (会话: {}, ID: {})", e, session_id, packet.header.message_id);
                            }
                        }
                    }
                    crate::packet::PacketType::Response => {
                        // 🎯 新增：处理响应包 - 完成服务端发起的请求
                        let message_id = packet.header.message_id;
                        tracing::debug!("📥 TransportServer 收到会话 {} 的响应包 (ID: {})", session_id, message_id);
                        
                        if self.request_tracker.complete(message_id, packet.clone()) {
                            tracing::debug!("✅ 成功完成服务端请求 (ID: {})", message_id);
                        } else {
                            tracing::warn!("⚠️ 收到未知响应包 (ID: {})，可能是超时或重复响应", message_id);
                            // 作为普通消息处理
                            let context = crate::event::TransportContext::new_oneway(
                                Some(session_id),
                                packet.header.message_id,
                                packet.header.biz_type,
                                if packet.ext_header.is_empty() { None } else { Some(packet.ext_header.clone()) },
                                packet.payload.clone(),
                            );
                            let event = crate::event::ServerEvent::MessageReceived { session_id, context };
                            match self.event_sender.send(event) {
                                Ok(receivers) => {
                                    tracing::debug!("✅ 未知响应包作为普通消息发送成功，接收者数量: {}", receivers);
                                }
                                Err(e) => {
                                    tracing::error!("❌ 未知响应包作为普通消息发送失败: {:?}", e);
                                }
                            }
                        }
                    }
                    _ => {
                        // 其他类型的数据包作为普通消息处理
                        tracing::debug!("📦 处理其他类型数据包 (类型: {:?}, ID: {})", packet.header.packet_type, packet.header.message_id);
                        let context = crate::event::TransportContext::new_oneway(
                            Some(session_id),
                            packet.header.message_id,
                            packet.header.biz_type,
                            if packet.ext_header.is_empty() { None } else { Some(packet.ext_header.clone()) },
                            packet.payload.clone(),
                        );
                        let event = crate::event::ServerEvent::MessageReceived { session_id, context };
                        match self.event_sender.send(event) {
                            Ok(receivers) => {
                                tracing::debug!("✅ 其他类型数据包发送成功，接收者数量: {}", receivers);
                            }
                            Err(e) => {
                                tracing::error!("❌ 其他类型数据包发送失败: {:?}", e);
                            }
                        }
                    }
                }
            }
            crate::event::TransportEvent::MessageSent { packet_id } => {
                tracing::debug!("📤 处理MessageSent事件 (ID: {})", packet_id);
                let event = crate::event::ServerEvent::MessageSent { session_id, message_id: packet_id };
                match self.event_sender.send(event) {
                    Ok(receivers) => {
                        tracing::debug!("✅ MessageSent事件发送成功，接收者数量: {}", receivers);
                    }
                    Err(e) => {
                        tracing::error!("❌ MessageSent事件发送失败: {:?}", e);
                    }
                }
            }
            crate::event::TransportEvent::ConnectionClosed { reason } => {
                tracing::debug!("🔌 处理ConnectionClosed事件，原因: {:?}", reason);
                let event = crate::event::ServerEvent::ConnectionClosed { session_id, reason };
                match self.event_sender.send(event) {
                    Ok(receivers) => {
                        tracing::debug!("✅ ConnectionClosed事件发送成功，接收者数量: {}", receivers);
                    }
                    Err(e) => {
                        tracing::error!("❌ ConnectionClosed事件发送失败: {:?}", e);
                    }
                }
                // 清理会话
                let _ = self.remove_session(session_id).await;
            }
            crate::event::TransportEvent::TransportError { error } => {
                tracing::debug!("❌ 处理TransportError事件: {:?}", error);
                let event = crate::event::ServerEvent::TransportError { 
                    session_id: Some(session_id), 
                    error 
                };
                match self.event_sender.send(event) {
                    Ok(receivers) => {
                        tracing::debug!("✅ TransportError事件发送成功，接收者数量: {}", receivers);
                    }
                    Err(e) => {
                        tracing::error!("❌ TransportError事件发送失败: {:?}", e);
                    }
                }
            }
            // 其他事件暂时忽略或记录
            _ => {
                tracing::trace!("📝 TransportServer 忽略事件: {:?}", transport_event);
            }
        }
    }

    /// 🎯 停止服务端
    pub async fn stop(&self) {
        tracing::info!("🛑 停止 TransportServer");
        self.is_running.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clone for TransportServer {
    fn clone(&self) -> Self {
        // 克隆协议配置 - 使用 clone_server_dyn()
        let mut cloned_configs = std::collections::HashMap::new();
        for (name, config) in &self.protocol_configs {
            cloned_configs.insert(name.clone(), config.clone_server_dyn());
        }
        
        Self {
            config: self.config.clone(),
            transports: self.transports.clone(),
            session_id_generator: self.session_id_generator.clone(),
            stats: self.stats.clone(),
            event_sender: self.event_sender.clone(),
            is_running: self.is_running.clone(),
            protocol_configs: cloned_configs,
            state_manager: self.state_manager.clone(),
            request_tracker: self.request_tracker.clone(),
            message_id_counter: std::sync::atomic::AtomicU32::new(20000), // 克隆时重新初始化
        }
    }
}

impl std::fmt::Debug for TransportServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportServer")
            .field("session_count", &self.transports.len())
            .field("config", &self.config)
            .finish()
    }
}
