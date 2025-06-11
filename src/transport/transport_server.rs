use std::sync::Arc;
use crate::{
    SessionId, TransportError, Packet, EventStream,
    transport::{
        config::TransportConfig,
        transport::Transport,
        expert_config::ExpertConfig,
        lockfree_enhanced::LockFreeHashMap,
    },
    command::TransportStats,
    event::TransportEvent,
    stream::StreamFactory,
    protocol::adapter::DynServerConfig,
};
use tokio::sync::broadcast;
use futures;

/// 🎯 多连接服务端传输层 - 使用 lockfree 数据结构管理多个 Transport 会话
pub struct TransportServer {
    /// 配置
    config: TransportConfig,
    /// 🎯 核心：会话到单连接 Transport 的映射 (使用 lockfree)
    sessions: Arc<LockFreeHashMap<SessionId, Transport>>,
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
    /// 创建新的多连接服务端
    pub async fn new(config: TransportConfig) -> Result<Self, TransportError> {
        tracing::info!("🚀 创建 TransportServer (使用 lockfree 管理)");
        
        // 创建事件广播通道，容量为 1000
        let (event_sender, _) = broadcast::channel(1000);
        
        Ok(Self {
            config,
            sessions: Arc::new(LockFreeHashMap::new()),
            session_id_generator: Arc::new(std::sync::atomic::AtomicU64::new(1)),
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
        tracing::info!("🚀 创建 TransportServer (带协议配置)");
        
        // 创建事件广播通道，容量为 1000
        let (event_sender, _) = broadcast::channel(1000);
        
        Ok(Self {
            config,
            sessions: Arc::new(LockFreeHashMap::new()),
            session_id_generator: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            stats: Arc::new(LockFreeHashMap::new()),
            event_sender,
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            protocol_configs,
        })
    }
    
    /// 🎯 核心方法：向指定会话发送数据
    pub async fn send_to_session(&self, session_id: SessionId, packet: Packet) -> Result<(), TransportError> {
        if let Some(transport) = self.sessions.get(&session_id) {
            tracing::debug!("📤 TransportServer 向会话 {} 发送数据包", session_id);
            transport.send(packet).await
        } else {
            Err(TransportError::connection_error(
                format!("Session {} not found", session_id), 
                false
            ))
        }
    }
    
    /// 🎯 添加新会话 (lockfree)
    pub async fn add_session(&self, transport: Transport) -> SessionId {
        let session_id = self.generate_session_id();
        
        // 使用 lockfree 插入
        self.sessions.insert(session_id, transport);
        
        // 初始化统计信息
        self.stats.insert(session_id, TransportStats::new());
        
        tracing::info!("✅ TransportServer 添加会话: {}", session_id);
        session_id
    }
    
    /// 🎯 移除会话 (lockfree)
    pub async fn remove_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        if let Ok(Some(_)) = self.sessions.remove(&session_id) {
            let _ = self.stats.remove(&session_id);
            tracing::info!("🗑️ TransportServer 移除会话: {}", session_id);
            Ok(())
        } else {
            Err(TransportError::connection_error(
                format!("Session {} not found for removal", session_id),
                false
            ))
        }
    }
    
    /// 🎯 广播消息到所有会话 (lockfree)
    pub async fn broadcast(&self, packet: Packet) -> Result<(), TransportError> {
        let success_count = 0;
        let error_count = 0;
        
        // 收集会话以避免生命周期问题
        let mut sessions_to_broadcast = Vec::new();
        let _ = self.sessions.for_each(|session_id, transport| {
            sessions_to_broadcast.push((*session_id, transport.clone()));
        });
        
        // 为每个会话创建广播任务
        for (session_id, transport) in sessions_to_broadcast {
            let packet_clone = packet.clone();
            tokio::spawn(async move {
                match transport.send(packet_clone).await {
                    Ok(()) => {
                        tracing::debug!("📤 广播成功 -> 会话 {}", session_id);
                    }
                    Err(e) => {
                        tracing::warn!("❌ 广播失败 -> 会话 {}: {:?}", session_id, e);
                    }
                }
            });
        }
        
        tracing::info!("📡 TransportServer 广播完成");
        Ok(())
    }
    
    /// 🎯 获取活跃会话列表 (lockfree)
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        let mut sessions = Vec::new();
        
        let _ = self.sessions.for_each(|session_id, _| {
            sessions.push(*session_id);
        });
        
        sessions
    }
    
    /// �� 获取会话数量 (lockfree)
    pub async fn session_count(&self) -> usize {
        self.sessions.len()
    }
    
    /// 🎯 获取会话统计 (lockfree)
    pub async fn get_session_stats(&self, session_id: SessionId) -> Option<TransportStats> {
        self.stats.get(&session_id).map(|stats| stats.clone())
    }
    
    /// 🎯 获取所有统计信息 (lockfree)
    pub async fn get_all_stats(&self) -> std::collections::HashMap<SessionId, TransportStats> {
        let mut all_stats = std::collections::HashMap::new();
        
        let _ = self.stats.for_each(|session_id, stats| {
            all_stats.insert(*session_id, stats.clone());
        });
        
        all_stats
    }
    
    /// 生成新的会话ID
    fn generate_session_id(&self) -> SessionId {
        let id = self.session_id_generator.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SessionId::new(id)
    }

    /// 🎯 获取事件流 - 用于监听服务端事件
    pub fn events(&self) -> EventStream {
        let receiver = self.event_sender.subscribe();
        StreamFactory::event_stream(receiver)
    }

    /// 🎯 启动服务端监听 - 根据协议配置启动相应的监听器
    pub async fn serve(&self) -> Result<(), TransportError> {
        tracing::info!("🚀 TransportServer 开始服务");
        
        // 设置运行状态
        self.is_running.store(true, std::sync::atomic::Ordering::SeqCst);
        
        // 检查是否有协议配置
        if self.protocol_configs.is_empty() {
            tracing::warn!("⚠️ 没有配置协议，服务端无法启动监听");
            return Err(TransportError::config_error("protocols", "No protocols configured"));
        }
        
        tracing::info!("🌟 启动 {} 个协议服务器", self.protocol_configs.len());
        
        // 创建监听任务的向量
        let mut listen_tasks = Vec::new();
        
        // 为每个协议配置启动服务器
        tracing::info!("🔍 开始遍历协议配置...");
        for (protocol_name, protocol_config) in &self.protocol_configs {
            tracing::info!("🔧 处理协议: {}", protocol_name);
            
            let address = self.get_protocol_bind_address(protocol_config);
            tracing::info!("📍 协议 {} 的绑定地址: {}", protocol_name, address);
            
            // 发送启动事件
            let start_event = TransportEvent::ServerStarted { address };
            let _ = self.event_sender.send(start_event);
            tracing::info!("📨 已发送 {} 协议启动事件", protocol_name);
            
            tracing::info!("🌐 {} 协议监听启动: {}", protocol_name, address);
            
            // 🔧 使用 DynServerConfig 的动态构建方法，实现真正的协议无关
            tracing::info!("🔧 使用 DynServerConfig 动态方法构建服务器");
            
            match protocol_config.build_server_dyn().await {
                Ok(server) => {
                    tracing::info!("✅ {} 服务器构建成功", protocol_name);
                    tracing::info!("🚀 开始创建 {} 监听任务...", protocol_name);
                    
                    match self.start_protocol_listener(server, protocol_name.clone()).await {
                        Ok(listener_task) => {
                            tracing::info!("✅ {} 监听任务创建成功", protocol_name);
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
            
            tracing::info!("✅ 协议 {} 处理完成", protocol_name);
        }
        
        tracing::info!("🎯 所有协议服务器启动完成，等待连接...");
        tracing::info!("📊 总共创建了 {} 个监听任务", listen_tasks.len());
        
        if listen_tasks.is_empty() {
            tracing::error!("❌ 没有成功创建任何监听任务");
            return Err(TransportError::config_error("server", "No listening tasks created"));
        }
        
        // 🔧 关键修复：参考旧版本，逐个等待任务完成而不是 join_all
        // 这确保服务器一直运行，除非出现错误
        tracing::info!("🔄 开始等待监听任务...");
        for (index, task) in listen_tasks.into_iter().enumerate() {
            tracing::info!("⏳ 等待第 {} 个监听任务完成...", index + 1);
            match task.await {
                Ok(()) => {
                    tracing::info!("✅ 第 {} 个监听任务正常完成", index + 1);
                }
                Err(e) => {
                    tracing::error!("❌ 第 {} 个监听任务被取消: {:?}", index + 1, e);
                    // 发送服务端停止事件
                    let stop_event = TransportEvent::ServerStopped;
                    let _ = self.event_sender.send(stop_event);
                    return Err(TransportError::config_error("server", "Listener task cancelled"));
                }
            }
        }
        
        // 发送服务端停止事件
        let stop_event = TransportEvent::ServerStopped;
        let _ = self.event_sender.send(stop_event);
        
        tracing::info!("🛑 TransportServer 已停止");
        Ok(())
    }

    /// 🎯 启动协议监听器 - 通用方法
    async fn start_protocol_listener<S>(&self, mut server: S, protocol_name: String) -> Result<tokio::task::JoinHandle<()>, TransportError>
    where
        S: crate::protocol::Server + 'static,
    {
        tracing::info!("🔧 即将创建 {} 协议监听任务", protocol_name);
        
        let server_clone = self.clone();
        let transport_config = self.config.clone();
        let protocol_name_for_log = protocol_name.clone();
        
        tracing::info!("📋 准备监听任务参数:");
        tracing::info!("   - 协议: {}", protocol_name);
        tracing::info!("   - 配置: {:?}", transport_config);
        
        let task = tokio::spawn(async move {
            tracing::info!("🚀 {} 监听任务已启动，开始执行", protocol_name);
            
            // 首先检查服务器的本地地址
            tracing::info!("🔍 {} 正在获取服务器本地地址...", protocol_name);
            match server.local_addr() {
                Ok(addr) => {
                    tracing::info!("✅ {} 服务器监听地址确认: {}", protocol_name, addr);
                }
                Err(e) => {
                    tracing::error!("❌ {} 服务器地址获取失败: {:?}", protocol_name, e);
                    tracing::error!("💥 {} 监听任务因地址获取失败而退出", protocol_name);
                    return;
                }
            }
            
            let mut accept_count = 0u64;
            tracing::info!("🔄 {} 进入连接接受循环", protocol_name);
            
            // 🔧 关键修复：参考旧版本，使用无限循环而不是条件循环
            // 真正的服务器监听应该一直运行，除非出现致命错误
            loop {
                tracing::debug!("🔄 {} 等待连接... (接受计数: {})", protocol_name, accept_count);
                
                // 在每次 accept 前添加日志
                tracing::debug!("🎣 {} 调用 server.accept()...", protocol_name);
                match server.accept().await {
                    Ok(connection) => {
                        accept_count += 1;
                        tracing::info!("🎉 {} accept 成功! 连接 #{}", protocol_name, accept_count);
                        
                        // 获取连接信息
                        let mut connection_info = connection.connection_info();
                        let peer_addr = connection_info.peer_addr;
                        
                        tracing::info!("🔗 新的 {} 连接 #{}: {}", protocol_name, accept_count, peer_addr);
                        
                        // 生成新的会话ID
                        let session_id = server_clone.generate_session_id();
                        tracing::info!("🆔 为 {} 连接生成会话ID: {}", protocol_name, session_id);
                        
                        // 创建 Transport 并添加到会话管理
                        tracing::info!("🚧 为 {} 连接创建 Transport...", protocol_name);
                        match Transport::new(transport_config.clone()).await {
                            Ok(transport) => {
                                tracing::info!("✅ {} Transport 创建成功", protocol_name);
                                let actual_session_id = server_clone.add_session(transport).await;
                                tracing::info!("✅ {} 会话创建成功: {} (来自 {})", protocol_name, actual_session_id, peer_addr);
                                
                                // 🔧 修复：更新 connection_info 中的 session_id 为实际分配的会话ID
                                connection_info.session_id = actual_session_id;
                                
                                // 🚀 启动简单的消息接收循环
                                tracing::info!("🚀 为会话 {} 启动消息接收循环", actual_session_id);
                                
                                let event_sender = server_clone.event_sender.clone();
                                let session_id = actual_session_id;
                                
                                // 启动消息接收任务
                                let _recv_task = tokio::spawn(async move {
                                    tracing::info!("📥 消息接收循环开始: {}", session_id);
                                    
                                    // 将 connection 转换为 mutable 以便调用 receive
                                    let mut connection = connection;
                                    
                                    loop {
                                        match connection.receive().await {
                                            Ok(Some(packet)) => {
                                                tracing::info!("📥 收到消息: {} bytes (会话: {})", packet.payload.len(), session_id);
                                                
                                                // 发送 MessageReceived 事件
                                                let event = TransportEvent::MessageReceived {
                                                    session_id,
                                                    packet: packet.clone(),
                                                };
                                                
                                                if let Err(e) = event_sender.send(event) {
                                                    tracing::error!("❌ 发送 MessageReceived 事件失败: {:?}", e);
                                                    break;
                                                } else {
                                                    tracing::debug!("✅ MessageReceived 事件已发送: {}", session_id);
                                                }
                                            }
                                            Ok(None) => {
                                                tracing::info!("🔚 连接已关闭: {}", session_id);
                                                break;
                                            }
                                            Err(e) => {
                                                tracing::error!("❌ 接收消息错误: {:?} (会话: {})", e, session_id);
                                                break;
                                            }
                                        }
                                    }
                                    
                                    tracing::info!("📥 消息接收循环结束: {}", session_id);
                                });
                                
                                tracing::info!("✅ 消息接收循环已启动: {}", actual_session_id);
                                
                                // 发送连接事件
                                let connect_event = TransportEvent::ConnectionEstablished { 
                                    session_id: actual_session_id,
                                    info: connection_info,
                                };
                                let _ = server_clone.event_sender.send(connect_event);
                                tracing::info!("📨 {} 连接事件已发送", protocol_name);
                            }
                            Err(e) => {
                                tracing::error!("❌ 创建 {} Transport 失败: {:?}", protocol_name, e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ {} 接受连接失败 (接受计数: {}): {:?}", protocol_name, accept_count, e);
                        
                        // 根据错误类型决定是否继续
                        match &e {
                            TransportError::Connection { reason, retryable } => {
                                if *retryable {
                                    if reason.contains("would block") || reason.contains("WouldBlock") {
                                        tracing::debug!("🔄 {} 无连接可接受，继续等待", protocol_name);
                                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                                        continue;
                                    } else if reason.contains("interrupted") || reason.contains("Interrupted") {
                                        tracing::warn!("⚠️ {} 接受连接被中断，继续监听", protocol_name);
                                        continue;
                                    } else {
                                        tracing::warn!("⚠️ {} 可重试的连接错误，继续监听: {}", protocol_name, reason);
                                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                        continue;
                                    }
                                } else {
                                    tracing::error!("💥 {} 不可重试的连接错误，停止监听: {}", protocol_name, reason);
                                    break;
                                }
                            }
                            _ => {
                                tracing::error!("💥 {} 其他类型错误，停止监听: {:?}", protocol_name, e);
                                break;
                            }
                        }
                    }
                }
            }
            
            tracing::info!("🛑 {} 服务器已停止 (共接受 {} 个连接)", protocol_name, accept_count);
        });
        
        tracing::info!("✅ {} 协议监听任务创建完成", protocol_name_for_log);
        tracing::info!("🎯 {} 任务句柄已准备就绪", protocol_name_for_log);
        Ok(task)
    }

    /// 🔧 内部方法：从协议配置中提取监听地址（使用 DynServerConfig 通用方法）
    fn get_protocol_bind_address(&self, protocol_config: &Box<dyn crate::protocol::adapter::DynServerConfig>) -> std::net::SocketAddr {
        // 🎯 使用 DynServerConfig 的通用方法，无需协议特定代码
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
            sessions: self.sessions.clone(),
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
            .field("session_count", &self.sessions.len())
            .field("config", &self.config)
            .finish()
    }
}
