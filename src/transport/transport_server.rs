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
    protocol::adapter::DynProtocolConfig,
};
use tokio::sync::broadcast;

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
    /// 协议配置 - 从 TransportServerBuilder 传入
    protocol_configs: std::collections::HashMap<String, Box<dyn DynProtocolConfig>>,
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
        protocol_configs: std::collections::HashMap<String, Box<dyn DynProtocolConfig>>
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
    
    /// 🎯 获取会话数量 (lockfree)
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
        
        // 为每个协议配置发送启动事件
        for (protocol_name, protocol_config) in &self.protocol_configs {
            let address = self.get_protocol_bind_address(protocol_config);
            
            let start_event = TransportEvent::ServerStarted { 
                address
            };
            let _ = self.event_sender.send(start_event);
            
            tracing::info!("🌐 {} 协议监听启动: {}", protocol_name, address);
        }
        
        // 如果没有配置协议，发送一个默认的启动事件
        if self.protocol_configs.is_empty() {
            let start_event = TransportEvent::ServerStarted { 
                address: "127.0.0.1:0".parse().unwrap() // 默认地址
            };
            let _ = self.event_sender.send(start_event);
            tracing::warn!("⚠️ 没有配置协议，服务端以空配置启动");
        }
        
        // 模拟服务端运行逻辑
        // TODO: 在实际实现中，这里应该是真正的网络监听循环
        while self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            
            // 检查是否有新连接 (模拟)
            // 这里可以添加实际的网络监听逻辑
        }
        
        // 发送服务端停止事件
        let stop_event = TransportEvent::ServerStopped;
        let _ = self.event_sender.send(stop_event);
        
        tracing::info!("🛑 TransportServer 已停止");
        Ok(())
    }

    /// 🎯 停止服务端
    pub async fn stop(&self) {
        tracing::info!("🛑 停止 TransportServer");
        self.is_running.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// 🔧 内部方法：从协议配置中提取监听地址
    fn get_protocol_bind_address(&self, protocol_config: &Box<dyn DynProtocolConfig>) -> std::net::SocketAddr {
        // 通过 as_any() 尝试向下转型到具体的协议配置类型
        match protocol_config.protocol_name() {
            "tcp" => {
                if let Some(tcp_config) = protocol_config.as_any().downcast_ref::<crate::protocol::TcpServerConfig>() {
                    tcp_config.bind_address
                } else {
                    "127.0.0.1:8000".parse().unwrap()
                }
            }
            "websocket" => {
                if let Some(ws_config) = protocol_config.as_any().downcast_ref::<crate::protocol::WebSocketServerConfig>() {
                    ws_config.bind_address
                } else {
                    "127.0.0.1:8001".parse().unwrap()
                }
            }
            "quic" => {
                if let Some(quic_config) = protocol_config.as_any().downcast_ref::<crate::protocol::QuicServerConfig>() {
                    quic_config.bind_address
                } else {
                    "127.0.0.1:8002".parse().unwrap()
                }
            }
            _ => {
                "127.0.0.1:8080".parse().unwrap()
            }
        }
    }
}

impl Clone for TransportServer {
    fn clone(&self) -> Self {
        // 克隆协议配置 - 使用 clone_dyn()
        let mut cloned_configs = std::collections::HashMap::new();
        for (name, config) in &self.protocol_configs {
            cloned_configs.insert(name.clone(), config.clone_dyn());
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
