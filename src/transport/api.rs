/// 统一API接口层
/// 
/// 提供高级的、协议无关的传输API

use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::HashMap;
use crate::{
    SessionId,
    command::{TransportStats, ConnectionInfo},
    error::TransportError,
    actor::{GenericActor, ActorHandle, ActorManager},
    protocol::{ProtocolAdapter, ProtocolConfig, ProtocolRegistry, Connection, ProtocolConnectionAdapter},
    stream::EventStream,
    packet::UnifiedPacket,
    adapters::create_standard_registry,
};
use super::config::TransportConfig;

/// 统一传输接口
/// 
/// 这是用户使用的主要接口，提供协议无关的传输功能
pub struct Transport {
    /// Actor管理器
    actor_manager: Arc<ActorManager>,
    /// 全局事件流
    #[allow(dead_code)]
    event_stream: EventStream,
    /// 会话ID生成器
    session_id_generator: Arc<AtomicU64>,
    /// 配置
    config: TransportConfig,
    /// 协议注册表
    protocol_registry: Arc<ProtocolRegistry>,
}

impl Transport {
    /// 创建新的传输实例
    pub async fn new(config: TransportConfig) -> Result<Self, TransportError> {
        let actor_manager = Arc::new(ActorManager::new());
        let event_stream = EventStream::new(actor_manager.global_events());
        
        // 创建标准协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        Ok(Self {
            actor_manager,
            event_stream,
            session_id_generator: Arc::new(AtomicU64::new(1)),
            config,
            protocol_registry,
        })
    }
    
    /// 添加新的连接
    pub async fn add_connection<A: ProtocolAdapter>(
        &self,
        adapter: A,
    ) -> Result<SessionId, TransportError> {
        let session_id = self.generate_session_id();
        
        // 创建Actor的命令通道
        let (command_tx, command_rx) = mpsc::channel(1024);
        
        // 使用全局事件发送器
        let global_event_tx = self.actor_manager.global_event_tx.clone();
        let global_event_rx = self.actor_manager.global_events();
        
        // 创建Actor
        let actor = GenericActor::new(
            adapter,
            session_id,
            command_rx,
            global_event_tx,
            A::Config::default_config(),
        );
        
        // 创建Actor句柄
        let handle = ActorHandle::new(
            command_tx,
            global_event_rx,
            session_id,
            Arc::new(tokio::sync::Mutex::new(0)),
        );
        
        // 添加到管理器
        self.actor_manager.add_actor(session_id, handle).await;
        
        // 启动Actor
        let actor_manager = self.actor_manager.clone();
        let session_id_for_cleanup = session_id;
        tokio::spawn(async move {
            if let Err(e) = actor.run().await {
                tracing::error!("Actor {} failed: {:?}", session_id_for_cleanup, e);
            }
            
            // 清理Actor
            actor_manager.remove_actor(&session_id_for_cleanup).await;
        });
        
        Ok(session_id)
    }
    
    /// 发送数据包到指定会话
    pub async fn send_to_session(
        &self,
        session_id: SessionId,
        packet: UnifiedPacket,
    ) -> Result<(), TransportError> {
        if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
            handle.send_packet(packet).await
        } else {
            Err(TransportError::SessionNotFound)
        }
    }
    
    /// 广播数据包到所有会话
    pub async fn broadcast(&self, packet: UnifiedPacket) -> Result<(), TransportError> {
        let sessions = self.actor_manager.active_sessions().await;
        let mut errors = Vec::new();
        
        for session_id in sessions {
            if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
                if let Err(e) = handle.send_packet(packet.clone()).await {
                    errors.push((session_id, e));
                }
            }
        }
        
        if !errors.is_empty() {
            Err(TransportError::BroadcastFailed(errors))
        } else {
            Ok(())
        }
    }
    
    /// 关闭指定会话
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
            handle.close().await?;
            self.actor_manager.remove_actor(&session_id).await;
            Ok(())
        } else {
            Err(TransportError::SessionNotFound)
        }
    }
    
    /// 获取所有活跃会话
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        self.actor_manager.active_sessions().await
    }
    
    /// 获取会话连接信息
    pub async fn session_info(&self, session_id: SessionId) -> Result<ConnectionInfo, TransportError> {
        if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
            handle.connection_info().await
        } else {
            Err(TransportError::SessionNotFound)
        }
    }
    
    /// 获取传输统计信息
    pub async fn stats(&self) -> Result<HashMap<SessionId, TransportStats>, TransportError> {
        let sessions = self.actor_manager.active_sessions().await;
        let mut stats = HashMap::new();
        
        for session_id in sessions {
            if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
                if let Ok(session_stats) = handle.stats().await {
                    stats.insert(session_id, session_stats);
                }
            }
        }
        
        Ok(stats)
    }
    
    /// 获取事件流
    pub fn events(&self) -> EventStream {
        EventStream::new(self.actor_manager.global_events())
    }
    
    /// 获取特定会话的事件流
    pub fn session_events(&self, session_id: SessionId) -> EventStream {
        EventStream::with_session_filter(self.actor_manager.global_events(), session_id)
    }
    
    /// 生成新的会话ID
    fn generate_session_id(&self) -> SessionId {
        self.session_id_generator.fetch_add(1, Ordering::SeqCst)
    }
    
    /// 基于URI连接到远程服务 (新的模块化API)
    pub async fn connect(&self, uri: &str) -> Result<SessionId, TransportError> {
        let connection = self.protocol_registry.create_connection(uri, None).await?;
        self.add_protocol_connection(connection).await
    }
    
    /// 基于URI和配置连接到远程服务
    pub async fn connect_with_config(
        &self, 
        uri: &str, 
        config: Box<dyn std::any::Any + Send + Sync>
    ) -> Result<SessionId, TransportError> {
        let connection = self.protocol_registry.create_connection(uri, Some(config)).await?;
        self.add_protocol_connection(connection).await
    }
    
    /// 启动协议服务器 (新的模块化API)
    pub async fn listen(&self, protocol: &str, bind_addr: &str) -> Result<SessionId, TransportError> {
        let server = self.protocol_registry.create_server(bind_addr, protocol, None).await?;
        self.add_protocol_server(server).await
    }
    
    /// 启动协议服务器并指定配置
    pub async fn listen_with_config(
        &self, 
        protocol: &str, 
        bind_addr: &str,
        config: Box<dyn std::any::Any + Send + Sync>
    ) -> Result<SessionId, TransportError> {
        let server = self.protocol_registry.create_server(bind_addr, protocol, Some(config)).await?;
        self.add_protocol_server(server).await
    }
    
    /// 获取协议注册表的引用
    pub fn protocol_registry(&self) -> &ProtocolRegistry {
        &self.protocol_registry
    }
    
    /// 列出所有已注册的协议
    pub async fn list_protocols(&self) -> Vec<String> {
        self.protocol_registry.list_protocols().await
    }
    
    /// 添加协议连接到Actor管理
    async fn add_protocol_connection(&self, mut connection: Box<dyn Connection>) -> Result<SessionId, TransportError> {
        let session_id = self.generate_session_id();
        connection.set_session_id(session_id);
        
        // 创建一个适配器包装器来兼容现有的add_connection方法
        let adapter = ProtocolConnectionAdapter::new(connection);
        self.add_connection(adapter).await
    }
    
    /// 添加协议服务器并开始接受连接
    async fn add_protocol_server(&self, mut server: Box<dyn crate::protocol::Server>) -> Result<SessionId, TransportError> {
        let session_id = self.generate_session_id();
        let transport = self.clone();
        
        tracing::debug!("启动协议服务器接受循环 (服务器会话ID: {})", session_id);
        
        // 启动服务器接受循环
        tokio::spawn(async move {
            tracing::debug!("协议服务器接受循环已启动，等待客户端连接...");
            
            loop {
                tracing::debug!("等待新的客户端连接...");
                
                match server.accept().await {
                    Ok(mut connection) => {
                        let conn_session_id = transport.generate_session_id();
                        tracing::info!("协议服务器接受到新连接 (连接会话ID: {})", conn_session_id);
                        
                        connection.set_session_id(conn_session_id);
                        
                        let adapter = ProtocolConnectionAdapter::new(connection);
                        match transport.add_connection(adapter).await {
                            Ok(_) => {
                                tracing::info!("成功添加协议连接到传输层 (会话ID: {})", conn_session_id);
                            }
                            Err(e) => {
                                tracing::error!("添加协议连接失败 (会话ID: {}): {:?}", conn_session_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("协议服务器接受连接时出错: {:?}", e);
                        break;
                    }
                }
            }
            
            tracing::warn!("协议服务器接受循环已退出");
        });
        
        Ok(session_id)
    }
}

/// 传输构建器
/// 
/// 用于创建配置好的传输实例
pub struct TransportBuilder {
    config: TransportConfig,
}

impl TransportBuilder {
    /// 创建新的传输构建器
    pub fn new() -> Self {
        Self {
            config: TransportConfig::default(),
        }
    }
    
    /// 设置配置
    pub fn config(mut self, config: TransportConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 构建传输实例
    pub async fn build(self) -> Result<Transport, TransportError> {
        self.config.validate()
            .map_err(|e| TransportError::ProtocolConfiguration(format!("Invalid config: {:?}", e)))?;
        
        Transport::new(self.config).await
    }
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 连接管理器
/// 
/// 提供连接的高级管理功能
pub struct ConnectionManager {
    transport: Transport,
}

impl ConnectionManager {
    /// 创建新的连接管理器
    pub fn new(transport: Transport) -> Self {
        Self { transport }
    }
    
    /// 创建TCP连接
    pub async fn create_tcp_connection(
        &self,
        addr: std::net::SocketAddr,
    ) -> Result<SessionId, TransportError> {
        use crate::adapters::tcp::TcpClientBuilder;
        use crate::protocol::TcpConfig;
        
        let config = TcpConfig::default();
        let adapter = TcpClientBuilder::new()
            .target_address(addr)
            .config(config)
            .connect()
            .await
            .map_err(|e| TransportError::Connection(format!("TCP connection failed: {:?}", e)))?;
        
        self.transport.add_connection(adapter).await
    }
    
    /// 创建WebSocket连接
    pub async fn create_websocket_connection(
        &self,
        url: &str,
    ) -> Result<SessionId, TransportError> {
        use crate::adapters::websocket::{WebSocketClientBuilder};
        use crate::protocol::WebSocketConfig;
        
        let config = WebSocketConfig::default();
        let adapter = WebSocketClientBuilder::new()
            .target_url(url)
            .config(config)
            .connect()
            .await
            .map_err(|e| TransportError::Connection(format!("WebSocket connection failed: {:?}", e)))?;
        
        self.transport.add_connection(adapter).await
    }
    
    /// 创建QUIC连接
    pub async fn create_quic_connection(
        &self,
        addr: std::net::SocketAddr,
    ) -> Result<SessionId, TransportError> {
        use crate::adapters::quic::{QuicClientBuilder};
        use crate::protocol::QuicConfig;
        
        let config = QuicConfig::default();
        let adapter = QuicClientBuilder::new()
            .target_address(addr)
            .config(config)
            .connect()
            .await
            .map_err(|e| TransportError::Connection(format!("QUIC connection failed: {:?}", e)))?;
        
        self.transport.add_connection(adapter).await
    }
    
    /// 获取内部传输实例的引用
    pub fn transport(&self) -> &Transport {
        &self.transport
    }
}

/// 服务器管理器
/// 
/// 管理多协议服务器
pub struct ServerManager {
    transport: Transport,
    servers: Arc<tokio::sync::Mutex<HashMap<String, ServerHandle>>>,
}

/// 服务器句柄
pub enum ServerHandle {
    Tcp(crate::adapters::tcp::TcpServer),
    WebSocket(crate::adapters::websocket::WebSocketServer),
    Quic(crate::adapters::quic::QuicServer),
}

impl ServerManager {
    /// 创建新的服务器管理器
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            servers: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }
    
    /// 启动TCP服务器
    pub async fn start_tcp_server(
        &self,
        name: String,
        addr: std::net::SocketAddr,
    ) -> Result<(), TransportError> {
        use crate::adapters::tcp::TcpServerBuilder;
        use crate::protocol::TcpConfig;
        
        let config = TcpConfig::default();
        let server = TcpServerBuilder::new()
            .bind_address(addr)
            .config(config.clone())
            .build()
            .await
            .map_err(|e| TransportError::Configuration(format!("Failed to start TCP server: {:?}", e)))?;
        
        // 启动接受循环
        let transport = self.transport.clone();
        let servers = self.servers.clone();
        let name_for_task = name.clone();
        
        // 先存储一个占位符，等spawn完成后再更新
        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.clone(), ServerHandle::Tcp(server));
        }
        
        // 重新创建server用于spawn（临时解决方案）
        let server_for_spawn = TcpServerBuilder::new()
            .bind_address(addr)
            .config(config)
            .build()
            .await
            .map_err(|e| TransportError::Configuration(format!("Failed to start TCP server: {:?}", e)))?;
        
        tokio::spawn(async move {
            let mut server = server_for_spawn;
            loop {
                match server.accept().await {
                    Ok(adapter) => {
                        if let Err(e) = transport.add_connection(adapter).await {
                            tracing::error!("Failed to add TCP connection: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("TCP server accept error: {:?}", e);
                        break;
                    }
                }
            }
            
            // 从服务器列表中移除
            servers.lock().await.remove(&name_for_task);
        });
        
        Ok(())
    }
    
    /// 启动WebSocket服务器
    pub async fn start_websocket_server(
        &self,
        name: String,
        addr: std::net::SocketAddr,
    ) -> Result<(), TransportError> {
        use crate::adapters::websocket::WebSocketServerBuilder;
        use crate::protocol::WebSocketConfig;
        
        let config = WebSocketConfig::default();
        let server = WebSocketServerBuilder::new()
            .bind_address(addr)
            .config(config.clone())
            .build()
            .await
            .map_err(|e| TransportError::Configuration(format!("Failed to start WebSocket server: {:?}", e)))?;
        
        // 启动接受循环
        let transport = self.transport.clone();
        let servers = self.servers.clone();
        let name_for_task = name.clone();
        
        // 先存储服务器句柄
        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.clone(), ServerHandle::WebSocket(server));
        }
        
        // 重新创建server用于spawn
        let server_for_spawn = WebSocketServerBuilder::new()
            .bind_address(addr)
            .config(config)
            .build()
            .await
            .map_err(|e| TransportError::Configuration(format!("Failed to start WebSocket server: {:?}", e)))?;
        
        tokio::spawn(async move {
            let mut server = server_for_spawn;
            loop {
                match server.accept().await {
                    Ok(adapter) => {
                        if let Err(e) = transport.add_connection(adapter).await {
                            tracing::error!("Failed to add WebSocket connection: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("WebSocket server accept error: {:?}", e);
                        break;
                    }
                }
            }
            
            // 从服务器列表中移除
            servers.lock().await.remove(&name_for_task);
        });
        
        Ok(())
    }
    
    /// 获取内部传输实例的引用
    pub fn transport(&self) -> &Transport {
        &self.transport
    }
}

// 为Transport实现Clone，使其可以在多个地方使用
impl Clone for Transport {
    fn clone(&self) -> Self {
        Self {
            actor_manager: self.actor_manager.clone(),
            event_stream: EventStream::new(self.actor_manager.global_events()),
            session_id_generator: self.session_id_generator.clone(),
            config: self.config.clone(),
            protocol_registry: self.protocol_registry.clone(),
        }
    }
} 