/// 多协议服务器演示 - 使用新的协议注册机制
/// 
/// 展示msgtrans统一架构的新协议注册系统：
/// 1. 统一的Transport API - connect() 和 listen()
/// 2. 协议模块化 - 支持TCP、WebSocket、QUIC和自定义协议
/// 3. 向后兼容 - 保持现有API的同时提供新的简化接口

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{Duration, sleep, timeout};

use msgtrans::unified::{
    Transport, TransportBuilder,
    packet::UnifiedPacket,
    error::TransportError,
    config::TransportConfig,
};
use futures::StreamExt;

/// 服务器统计信息
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub active_connections: usize,
    pub total_connections: u64,
    pub protocol_stats: HashMap<String, u64>, // 按协议统计连接数
    pub total_packets: u64,
    pub total_bytes: u64,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            active_connections: 0,
            total_connections: 0,
            protocol_stats: HashMap::new(),
            total_packets: 0,
            total_bytes: 0,
        }
    }
}

/// 客户端连接管理器
#[derive(Debug)]
pub struct ClientManager {
    tcp_sessions: Arc<RwLock<Vec<u64>>>,
    ws_sessions: Arc<RwLock<Vec<u64>>>,
    quic_sessions: Arc<RwLock<Vec<u64>>>,
}

impl ClientManager {
    pub fn new() -> Self {
        Self {
            tcp_sessions: Arc::new(RwLock::new(Vec::new())),
            ws_sessions: Arc::new(RwLock::new(Vec::new())),
            quic_sessions: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub async fn add_tcp_session(&self, session_id: u64) {
        self.tcp_sessions.write().await.push(session_id);
    }
    
    pub async fn add_ws_session(&self, session_id: u64) {
        self.ws_sessions.write().await.push(session_id);
    }
    
    pub async fn add_quic_session(&self, session_id: u64) {
        self.quic_sessions.write().await.push(session_id);
    }
    
    pub async fn get_all_sessions(&self) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
        let tcp = self.tcp_sessions.read().await.clone();
        let ws = self.ws_sessions.read().await.clone();
        let quic = self.quic_sessions.read().await.clone();
        (tcp, ws, quic)
    }
}

/// 统一多协议服务器 - 新架构版本
pub struct MultiProtocolServer {
    transport: Transport,
    stats: Arc<Mutex<ServerStats>>,
    client_manager: Arc<ClientManager>,
}

impl MultiProtocolServer {
    /// 创建新的多协议服务器
    pub async fn new() -> Result<Self, TransportError> {
        // 使用新的协议注册机制创建传输层
        let config = TransportConfig::default();
        let transport = TransportBuilder::new()
            .config(config)
            .build()
            .await?;
        
        let stats = Arc::new(Mutex::new(ServerStats::default()));
        let client_manager = Arc::new(ClientManager::new());
        
        // 显示已注册的协议
        let protocols = transport.list_protocols().await;
        println!("🔧 已注册的协议: {:?}", protocols);
        
        Ok(Self {
            transport,
            stats,
            client_manager,
        })
    }
    
    /// 启动多协议服务器
    pub async fn start(&self) -> Result<(), TransportError> {
        println!("🚀 启动多协议服务器（新架构版本）");
        println!("===============================");
        
        // 使用新的统一API启动多个协议服务器
        let servers = vec![
            ("TCP", "127.0.0.1:9001", None),
            ("WebSocket", "127.0.0.1:9002", None),
            ("QUIC", "127.0.0.1:9003", None),
        ];
        
        let mut server_handles = Vec::new();
        
        for (protocol_name, bind_addr, config) in servers {
            match self.start_protocol_server(protocol_name, bind_addr, config).await {
                Ok(session_id) => {
                    println!("✅ {} 服务器启动成功: {} (会话ID: {})", 
                             protocol_name, bind_addr, session_id);
                    server_handles.push((protocol_name, session_id));
                }
                Err(e) => {
                    println!("❌ {} 服务器启动失败: {:?}", protocol_name, e);
                    // 如果是端口占用，尝试其他端口
                    if let TransportError::ProtocolConfiguration(msg) = &e {
                        if msg.contains("Address already in use") {
                            println!("   💡 提示：端口 {} 被占用，请检查是否有其他程序在使用", 
                                   bind_addr.split(':').last().unwrap_or(""));
                        }
                    }
                }
            }
        }
        
        // 等待服务器稳定启动
        sleep(Duration::from_millis(500)).await;
        
        // 启动统计报告
        self.start_stats_reporter().await;
        
        // 模拟客户端连接和消息处理
        self.simulate_client_connections().await?;
        
        // 等待连接稳定
        sleep(Duration::from_secs(1)).await;
        
        // 发送测试消息
        self.send_test_messages_to_active_sessions().await?;
        
        // 保持服务器运行
        println!("\n🔄 服务器正在运行，按 Ctrl+C 退出...");
        tokio::signal::ctrl_c().await.map_err(|e| 
            TransportError::Connection(format!("Signal error: {}", e)))?;
        
        println!("\n👋 服务器正在关闭...");
        Ok(())
    }
    
    /// 使用统一API启动特定协议的服务器
    async fn start_protocol_server(
        &self,
        protocol_name: &str,
        bind_addr: &str,
        config: Option<Box<dyn std::any::Any + Send + Sync>>
    ) -> Result<u64, TransportError> {
        let protocol = protocol_name.to_lowercase();
        
        // 使用新的统一listen API
        let session_id = if let Some(config) = config {
            self.transport.listen_with_config(&protocol, bind_addr, config).await?
        } else {
            self.transport.listen(&protocol, bind_addr).await?
        };
        
        // 更新统计信息
        {
            let mut stats = self.stats.lock().await;
            stats.total_connections += 1;
            *stats.protocol_stats.entry(protocol_name.to_string()).or_insert(0) += 1;
        }
        
        // 启动消息处理任务
        let transport = self.transport.clone();
        let stats = self.stats.clone();
        let protocol_name = protocol_name.to_string();
        
        tokio::spawn(async move {
            let mut events = transport.events();
            
            loop {
                match events.next().await {
                    Some(event) => {
                        if let Err(e) = Self::handle_transport_event(
                            event, 
                            &transport, 
                            &stats, 
                            &protocol_name
                        ).await {
                            println!("❌ 处理{}事件时出错: {:?}", protocol_name, e);
                        }
                    }
                    None => {
                        println!("❌ {}事件流结束", protocol_name);
                        break;
                    }
                }
            }
        });
        
        Ok(session_id)
    }
    
    /// 处理传输事件（新架构版本）
    async fn handle_transport_event(
        event: msgtrans::unified::event::TransportEvent,
        transport: &Transport,
        stats: &Arc<Mutex<ServerStats>>,
        protocol_name: &str,
    ) -> Result<(), TransportError> {
        use msgtrans::unified::event::TransportEvent;
        
        match event {
            TransportEvent::PacketReceived { session_id, packet } => {
                println!("📨 {}服务器收到消息: 会话{}, 类型{:?}, ID{}", 
                         protocol_name, session_id, packet.packet_type, packet.message_id);
                
                // 更新统计
                {
                    let mut stats = stats.lock().await;
                    stats.total_packets += 1;
                    stats.total_bytes += packet.payload.len() as u64;
                }
                
                // 创建回显响应
                if let Some(content) = packet.payload_as_string() {
                    let response = UnifiedPacket::echo(
                        packet.message_id, 
                        format!("{}回显: {}", protocol_name, content).as_str()
                    );
                    
                    if let Err(e) = transport.send_to_session(session_id, response).await {
                        println!("❌ 发送回显响应失败: {:?}", e);
                    } else {
                        println!("📤 {}服务器发送回显响应", protocol_name);
                    }
                }
            }
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🔗 {}新连接建立: 会话{}, 地址{:?}", 
                         protocol_name, session_id, info.peer_addr);
                
                // 更新活跃连接数
                {
                    let mut stats = stats.lock().await;
                    stats.active_connections += 1;
                }
            }
            TransportEvent::ConnectionClosed { session_id, reason } => {
                println!("❌ {}连接关闭: 会话{}, 原因: {:?}", 
                         protocol_name, session_id, reason);
                
                // 更新活跃连接数
                {
                    let mut stats = stats.lock().await;
                    if stats.active_connections > 0 {
                        stats.active_connections -= 1;
                    }
                }
            }
            _ => {
                println!("📡 {}其他事件: {:?}", protocol_name, event);
            }
        }
        
        Ok(())
    }
    
    /// 启动统计报告任务
    async fn start_stats_reporter(&self) {
        let stats = self.stats.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                let stats = stats.lock().await;
                println!("\n📊 服务器统计信息:");
                println!("   活跃连接: {}", stats.active_connections);
                println!("   总连接数: {}", stats.total_connections);
                println!("   协议分布: {:?}", stats.protocol_stats);
                println!("   总数据包: {}", stats.total_packets);
                println!("   总字节数: {}", stats.total_bytes);
                println!();
            }
        });
    }
    
    /// 模拟客户端连接（展示新的connect API）
    async fn simulate_client_connections(&self) -> Result<(), TransportError> {
        println!("\n🎭 开始模拟客户端连接...");
        
        // 模拟TCP客户端连接
        let tcp_sessions = self.simulate_tcp_clients().await?;
        for session_id in &tcp_sessions {
            self.client_manager.add_tcp_session(*session_id).await;
        }
        println!("✅ 模拟了 {} 个TCP客户端连接", tcp_sessions.len());
        
        // 模拟WebSocket客户端连接  
        let ws_sessions = self.simulate_websocket_clients().await?;
        for session_id in &ws_sessions {
            self.client_manager.add_ws_session(*session_id).await;
        }
        println!("✅ 模拟了 {} 个WebSocket客户端连接", ws_sessions.len());
        
        // 模拟QUIC客户端连接
        let quic_sessions = self.simulate_quic_clients().await?;
        for session_id in &quic_sessions {
            self.client_manager.add_quic_session(*session_id).await;
        }
        println!("✅ 模拟了 {} 个QUIC客户端连接", quic_sessions.len());
        
        Ok(())
    }
    
    /// 模拟TCP客户端（展示统一connect API）
    async fn simulate_tcp_clients(&self) -> Result<Vec<u64>, TransportError> {
        let mut sessions = Vec::new();
        
        for i in 1..=2 {
            // 使用新的统一connect API - 支持URI格式
            match timeout(Duration::from_secs(5), self.transport.connect("tcp://127.0.0.1:9001")).await {
                Ok(Ok(session_id)) => {
                    println!("🔗 TCP客户端{} 连接成功 (会话ID: {})", i, session_id);
                    sessions.push(session_id);
                }
                Ok(Err(e)) => {
                    println!("❌ TCP客户端{} 连接失败: {:?}", i, e);
                }
                Err(_) => {
                    println!("❌ TCP客户端{} 连接超时", i);
                }
            }
            
            sleep(Duration::from_millis(200)).await;
        }
        
        Ok(sessions)
    }
    
    /// 模拟WebSocket客户端
    async fn simulate_websocket_clients(&self) -> Result<Vec<u64>, TransportError> {
        let mut sessions = Vec::new();
        
        for i in 1..=3 {
            // 使用统一API连接WebSocket
            match timeout(Duration::from_secs(5), self.transport.connect("ws://127.0.0.1:9002")).await {
                Ok(Ok(session_id)) => {
                    println!("🌐 WebSocket客户端{} 连接成功 (会话ID: {})", i, session_id);
                    sessions.push(session_id);
                }
                Ok(Err(e)) => {
                    println!("❌ WebSocket客户端{} 连接失败: {:?}", i, e);
                }
                Err(_) => {
                    println!("❌ WebSocket客户端{} 连接超时", i);
                }
            }
            
            sleep(Duration::from_millis(200)).await;
        }
        
        Ok(sessions)
    }
    
    /// 模拟QUIC客户端
    async fn simulate_quic_clients(&self) -> Result<Vec<u64>, TransportError> {
        let mut sessions = Vec::new();
        
        for i in 1..=2 {
            // 使用统一API连接QUIC
            match timeout(Duration::from_secs(5), self.transport.connect("quic://127.0.0.1:9003")).await {
                Ok(Ok(session_id)) => {
                    println!("⚡ QUIC客户端{} 连接成功 (会话ID: {})", i, session_id);
                    sessions.push(session_id);
                }
                Ok(Err(e)) => {
                    println!("❌ QUIC客户端{} 连接失败: {:?}", i, e);
                }
                Err(_) => {
                    println!("❌ QUIC客户端{} 连接超时", i);
                }
            }
            
            sleep(Duration::from_millis(200)).await;
        }
        
        Ok(sessions)
    }
    
    /// 发送测试消息到活跃会话
    async fn send_test_messages_to_active_sessions(&self) -> Result<(), TransportError> {
        let (tcp_sessions, ws_sessions, quic_sessions) = self.client_manager.get_all_sessions().await;
        
        if tcp_sessions.is_empty() && ws_sessions.is_empty() && quic_sessions.is_empty() {
            println!("⚠️  没有活跃的客户端连接，跳过消息发送");
            return Ok(());
        }
        
        println!("\n📤 开始发送测试消息...");
        println!("   TCP会话: {:?}", tcp_sessions);
        println!("   WebSocket会话: {:?}", ws_sessions);
        println!("   QUIC会话: {:?}", quic_sessions);
        
        let test_messages = vec![
            "Hello from multiprotocol server!",
            "你好，这是中文测试消息！",
            "JSON测试: {\"type\":\"test\",\"data\":\"multiprotocol\"}",
        ];
        
        for (msg_id, message) in test_messages.iter().enumerate() {
            let packet = UnifiedPacket::data(msg_id as u32 + 1, message.as_bytes());
            
            // 发送到所有TCP连接
            for &session_id in &tcp_sessions {
                if let Err(e) = self.transport.send_to_session(session_id, packet.clone()).await {
                    println!("❌ 发送到TCP会话{}失败: {:?}", session_id, e);
                } else {
                    println!("✅ 发送消息到TCP会话{}: {}", session_id, message);
                }
            }
            
            // 发送到所有WebSocket连接
            for &session_id in &ws_sessions {
                if let Err(e) = self.transport.send_to_session(session_id, packet.clone()).await {
                    println!("❌ 发送到WebSocket会话{}失败: {:?}", session_id, e);
                } else {
                    println!("✅ 发送消息到WebSocket会话{}: {}", session_id, message);
                }
            }
            
            // 发送到所有QUIC连接
            for &session_id in &quic_sessions {
                if let Err(e) = self.transport.send_to_session(session_id, packet.clone()).await {
                    println!("❌ 发送到QUIC会话{}失败: {:?}", session_id, e);
                } else {
                    println!("✅ 发送消息到QUIC会话{}: {}", session_id, message);
                }
            }
            
            sleep(Duration::from_millis(300)).await;
        }
        
        // 发送心跳包
        let heartbeat = UnifiedPacket::heartbeat();
        println!("💓 发送心跳包到所有连接...");
        
        if let Err(e) = self.transport.broadcast(heartbeat).await {
            println!("❌ 广播心跳包失败: {:?}", e);
        } else {
            println!("✅ 心跳包广播成功");
        }
        
        Ok(())
    }
    
    /// 展示协议注册的扩展性
    pub async fn demonstrate_protocol_extensibility(&self) -> Result<(), TransportError> {
        println!("\n🔧 演示协议注册机制的扩展性:");
        
        // 获取协议注册表
        let registry = self.transport.protocol_registry();
        
        // 列出当前支持的协议
        let protocols = registry.list_protocols().await;
        println!("   当前协议: {:?}", protocols);
        
        // 列出支持的URL schemes
        let schemes = registry.list_schemes().await;
        println!("   支持的URI schemes: {:?}", schemes);
        
        // 演示如何添加自定义协议（这里只是展示API，实际需要实现CustomProtocolFactory）
        println!("   💡 可以通过 registry.register(CustomProtocolFactory::new()) 添加自定义协议");
        println!("   💡 新协议自动支持 transport.connect() 和 transport.listen() API");
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🌟 msgtrans 多协议服务器演示 - 新架构版本");
    println!("===========================================");
    println!("🚀 特性展示:");
    println!("   ✨ 统一的 connect() 和 listen() API");
    println!("   🔧 协议模块化和可扩展性"); 
    println!("   🌐 支持 TCP、WebSocket、QUIC");
    println!("   📡 统一的消息处理和事件系统");
    println!("   ⚡ 向后兼容现有API");
    println!();
    
    // 创建服务器
    let server = MultiProtocolServer::new().await?;
    
    // 演示协议扩展性
    server.demonstrate_protocol_extensibility().await?;
    
    // 启动服务器
    server.start().await?;
    
    Ok(())
} 