/// 服务端传输层实现
/// 
/// 提供多协议服务端支持，管理会话和连接

use std::sync::Arc;
use crate::{
    SessionId, TransportError, Packet, EventStream, TransportEvent,
    transport::{
        config::TransportConfig,
        lockfree_enhanced::LockFreeHashMap,
    },
    command::TransportStats,
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
    /// 🎯 核心：会话到连接的映射 (使用 lockfree)
    connections: Arc<LockFreeHashMap<SessionId, Arc<tokio::sync::Mutex<Box<dyn crate::protocol::Connection>>>>>,
    /// 会话ID生成器
    session_id_generator: Arc<std::sync::atomic::AtomicU64>,
    /// 服务端统计信息 (使用 lockfree)
    stats: Arc<LockFreeHashMap<SessionId, TransportStats>>,
    /// 事件广播器
    event_sender: broadcast::Sender<TransportEvent>,
    /// 是否正在运行
    is_running: Arc<std::sync::atomic::AtomicBool>,
    /// 🔧 协议配置 - 改为服务端专用配置
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynServerConfig>>,
}

impl TransportServer {
    /// 创建新的 TransportServer
    pub async fn new(config: TransportConfig) -> Result<Self, TransportError> {
        let (event_sender, _) = broadcast::channel(1000);
        
        Ok(Self {
            config,
            connections: Arc::new(LockFreeHashMap::new()),
            session_id_generator: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            stats: Arc::new(LockFreeHashMap::new()),
            event_sender,
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            protocol_configs: std::collections::HashMap::new(),
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
            connections: Arc::new(LockFreeHashMap::new()),
            session_id_generator: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            stats: Arc::new(LockFreeHashMap::new()),
            event_sender,
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            protocol_configs,
        })
    }

    /// 向指定会话发送数据包
    pub async fn send_to_session(&self, session_id: SessionId, packet: Packet) -> Result<(), TransportError> {
        tracing::debug!("📤 TransportServer 向会话 {} 发送数据包 (ID: {}, 大小: {} bytes)", 
            session_id, packet.message_id, packet.payload.len());
        
        if let Some(connection) = self.connections.get(&session_id) {
            let mut conn = connection.lock().await;
            
            // 🔧 关键修复：在发送前检查连接状态
            if !conn.is_connected() {
                tracing::warn!("⚠️ 会话 {} 连接已断开，跳过发送", session_id);
                // 清理已断开的连接
                drop(conn); // 释放锁
                let _ = self.remove_session(session_id).await;
                return Err(TransportError::connection_error("Connection closed", false));
            }
            
            tracing::debug!("🔍 会话 {} 连接状态正常，开始发送数据包", session_id);
            
            // 尝试发送数据包
            match conn.send(packet).await {
                Ok(()) => {
                    tracing::debug!("✅ 会话 {} TCP层发送成功 (TransportServer层确认)", session_id);
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("❌ 会话 {} TCP层发送失败: {:?}", session_id, e);
                    
                    // 🔧 关键修复：检查是否是连接相关错误
                    let error_msg = format!("{:?}", e);
                    if error_msg.contains("Broken pipe") || 
                       error_msg.contains("Connection reset") || 
                       error_msg.contains("Connection closed") ||
                       error_msg.contains("ECONNRESET") ||
                       error_msg.contains("EPIPE") {
                        tracing::warn!("⚠️ 会话 {} 连接已断开: {}", session_id, error_msg);
                        // 清理已断开的连接
                        drop(conn); // 释放锁
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

    /// 添加新会话
    pub async fn add_session(&self, connection: Box<dyn crate::protocol::Connection>) -> SessionId {
        let session_id = self.generate_session_id();
        let wrapped_connection = Arc::new(tokio::sync::Mutex::new(connection));
        self.connections.insert(session_id, wrapped_connection);
        self.stats.insert(session_id, TransportStats::new());
        
        tracing::info!("✅ TransportServer 添加会话: {}", session_id);
        session_id
    }

    /// 移除会话
    pub async fn remove_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        self.connections.remove(&session_id);
        self.stats.remove(&session_id);
        tracing::info!("🗑️ TransportServer 移除会话: {}", session_id);
        Ok(())
    }

    /// 广播消息到所有会话
    pub async fn broadcast(&self, packet: Packet) -> Result<(), TransportError> {
        let mut success_count = 0;
        let mut error_count = 0;
        
        // 使用 for_each 遍历连接
        let _ = self.connections.for_each(|session_id, connection| {
            // 这里需要异步处理，但 for_each 不支持异步
            // 所以我们先收集所有连接，然后处理
        });
        
        // 改为先收集所有会话ID，然后逐个处理
        let session_ids: Vec<SessionId> = self.connections.keys().unwrap_or_default();
        for session_id in session_ids {
            if let Some(connection) = self.connections.get(&session_id) {
                let mut conn = connection.lock().await;
                match conn.send(packet.clone()).await {
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
        self.connections.keys().unwrap_or_default()
    }

    /// 获取会话计数
    pub async fn session_count(&self) -> usize {
        self.connections.len()
    }

    /// 生成新的会话ID
    fn generate_session_id(&self) -> SessionId {
        let id = self.session_id_generator.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SessionId(id)
    }

    /// 获取事件流
    pub fn events(&self) -> EventStream {
        EventStream::new(self.event_sender.subscribe())
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
    async fn start_protocol_listener(&self, mut server: Box<dyn crate::protocol::Server>, protocol_name: String) -> Result<tokio::task::JoinHandle<()>, TransportError>
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
                        let connect_event = TransportEvent::ConnectionEstablished { 
                            session_id: actual_session_id,
                            info: connection_info,
                        };
                        let _ = server_clone.event_sender.send(connect_event);
                        tracing::info!("📨 {} 连接事件已发送", protocol_name);
                        
                        // 启动消息接收循环
                        let event_sender = server_clone.event_sender.clone();
                        let connections = server_clone.connections.clone();
                        let server_for_cleanup = server_clone.clone();
                        
                        tokio::spawn(async move {
                            tracing::info!("📥 消息接收循环开始: {}", actual_session_id);
                            
                            loop {
                                if let Some(connection) = connections.get(&actual_session_id) {
                                    let mut conn = connection.lock().await;
                                    match conn.receive().await {
                                        Ok(Some(packet)) => {
                                            tracing::info!("📥 收到消息: {} bytes (会话: {})", packet.payload.len(), actual_session_id);
                                            
                                            let event = TransportEvent::MessageReceived {
                                                session_id: actual_session_id,
                                                packet: packet.clone(),
                                            };
                                            
                                            if let Err(e) = event_sender.send(event) {
                                                tracing::error!("❌ 发送 MessageReceived 事件失败: {:?}", e);
                                                break;
                                            } else {
                                                tracing::debug!("✅ MessageReceived 事件已发送: {}", actual_session_id);
                                            }
                                        }
                                        Ok(None) => {
                                            tracing::info!("🔚 连接已关闭: {}", actual_session_id);
                                            break;
                                        }
                                        Err(e) => {
                                            // 🔧 修复：区分正常连接关闭和真正的错误
                                            let error_msg = format!("{:?}", e);
                                            if error_msg.contains("Connection reset") || 
                                               error_msg.contains("Connection closed") ||
                                               error_msg.contains("ConnectionReset") ||
                                               error_msg.contains("UnexpectedEof") ||
                                               error_msg.contains("Broken pipe") {
                                                tracing::info!("🔗 会话 {} 连接已被对端关闭: {}", actual_session_id, e);
                                            } else {
                                                tracing::error!("❌ 接收消息错误: {:?} (会话: {})", e, actual_session_id);
                                            }
                                            break;
                                        }
                                    }
                                } else {
                                    tracing::error!("❌ 会话 {} 的连接已不存在", actual_session_id);
                                    break;
                                }
                            }
                            
                            // 🔧 关键修复：消息接收循环结束时清理连接
                            tracing::info!("📥 消息接收循环结束，清理会话: {}", actual_session_id);
                            let _ = server_for_cleanup.remove_session(actual_session_id).await;
                            
                            // 发送连接关闭事件
                            let close_event = TransportEvent::ConnectionClosed { 
                                session_id: actual_session_id,
                                reason: crate::error::CloseReason::Error("Connection closed".to_string()),
                            };
                            let _ = event_sender.send(close_event);
                        });
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
            connections: self.connections.clone(),
            session_id_generator: self.session_id_generator.clone(),
            stats: self.stats.clone(),
            event_sender: self.event_sender.clone(),
            is_running: self.is_running.clone(),
            protocol_configs: cloned_configs,
        }
    }
}

impl std::fmt::Debug for TransportServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportServer")
            .field("session_count", &self.connections.len())
            .field("config", &self.config)
            .finish()
    }
}
