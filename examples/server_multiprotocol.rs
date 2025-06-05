/// 多协议服务器演示
/// 
/// 展示msgtrans统一架构如何同时支持TCP、WebSocket和QUIC协议
/// 提供统一的消息处理和回显功能

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{Duration, sleep};

/// 数据包类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PacketType {
    /// 心跳包
    Heartbeat = 0,
    /// 数据消息
    Data = 1,
    /// 控制消息
    Control = 2,
    /// 错误消息
    Error = 3,
    /// 认证消息
    Auth = 4,
    /// 回显消息（用于测试）
    Echo = 255,
}

impl From<u8> for PacketType {
    fn from(value: u8) -> Self {
        match value {
            0 => PacketType::Heartbeat,
            1 => PacketType::Data,
            2 => PacketType::Control,
            3 => PacketType::Error,
            4 => PacketType::Auth,
            255 => PacketType::Echo,
            _ => PacketType::Data,
        }
    }
}

impl From<PacketType> for u8 {
    fn from(packet_type: PacketType) -> Self {
        packet_type as u8
    }
}

/// 统一架构数据包
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedPacket {
    /// 数据包类型
    pub packet_type: PacketType,
    /// 消息ID（用于请求-响应匹配）
    pub message_id: u32,
    /// 负载数据
    pub payload: Vec<u8>,
}

impl UnifiedPacket {
    /// 创建新的数据包
    pub fn new(packet_type: PacketType, message_id: u32, payload: Vec<u8>) -> Self {
        Self {
            packet_type,
            message_id,
            payload,
        }
    }
    
    /// 创建数据消息包
    pub fn data(message_id: u32, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(PacketType::Data, message_id, payload.into())
    }
    
    /// 创建回显包
    pub fn echo(message_id: u32, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(PacketType::Echo, message_id, payload.into())
    }
    
    /// 创建心跳包
    pub fn heartbeat() -> Self {
        Self::new(PacketType::Heartbeat, 0, Vec::new())
    }
    
    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(9 + self.payload.len());
        
        // 写入包类型（1字节）
        buffer.push(self.packet_type.into());
        
        // 写入消息ID（4字节，大端序）
        buffer.extend_from_slice(&self.message_id.to_be_bytes());
        
        // 写入负载长度（4字节，大端序）
        buffer.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        
        // 写入负载
        buffer.extend_from_slice(&self.payload);
        
        buffer
    }
    
    /// 从字节反序列化
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 9 {
            return Err("数据太短".to_string());
        }
        
        // 读取包类型
        let packet_type = PacketType::from(data[0]);
        
        // 读取消息ID
        let message_id = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
        
        // 读取负载长度
        let payload_len = u32::from_be_bytes([data[5], data[6], data[7], data[8]]) as usize;
        
        // 检查数据完整性
        if data.len() != 9 + payload_len {
            return Err("数据长度不匹配".to_string());
        }
        
        // 读取负载
        let payload = data[9..].to_vec();
        
        Ok(Self {
            packet_type,
            message_id,
            payload,
        })
    }
    
    /// 获取负载的字符串表示
    pub fn payload_as_string(&self) -> Option<String> {
        String::from_utf8(self.payload.clone()).ok()
    }
}

/// 协议类型
#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolType {
    Tcp,
    WebSocket,
    Quic,
}

impl std::fmt::Display for ProtocolType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolType::Tcp => write!(f, "TCP"),
            ProtocolType::WebSocket => write!(f, "WebSocket"),
            ProtocolType::Quic => write!(f, "QUIC"),
        }
    }
}

/// 客户端连接信息
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: u64,
    pub protocol: ProtocolType,
    pub remote_addr: String,
    pub connected_at: SystemTime,
    pub packets_received: u64,
    pub packets_sent: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
}

/// 服务器统计信息
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub active_connections: usize,
    pub total_connections: u64,
    pub tcp_connections: usize,
    pub websocket_connections: usize,
    pub quic_connections: usize,
    pub total_packets: u64,
    pub total_bytes: u64,
}

/// 统一多协议服务器
pub struct MultiProtocolServer {
    clients: Arc<Mutex<HashMap<u64, ClientInfo>>>,
    next_client_id: Arc<Mutex<u64>>,
    stats: Arc<Mutex<ServerStats>>,
    message_tx: mpsc::UnboundedSender<(u64, UnifiedPacket)>,
    message_rx: Option<mpsc::UnboundedReceiver<(u64, UnifiedPacket)>>,
}

impl MultiProtocolServer {
    /// 创建新的多协议服务器
    pub fn new() -> Self {
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            next_client_id: Arc::new(Mutex::new(1)),
            stats: Arc::new(Mutex::new(ServerStats {
                active_connections: 0,
                total_connections: 0,
                tcp_connections: 0,
                websocket_connections: 0,
                quic_connections: 0,
                total_packets: 0,
                total_bytes: 0,
            })),
            message_tx,
            message_rx: Some(message_rx),
        }
    }
    
    /// 启动服务器
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 启动msgtrans多协议服务器");
        println!("=============================");
        
        // 获取消息接收器
        let mut message_rx = self.message_rx.take().unwrap();
        let clients = self.clients.clone();
        let stats = self.stats.clone();
        
        // 启动消息处理任务
        let message_handler = tokio::spawn(async move {
            while let Some((client_id, packet)) = message_rx.recv().await {
                Self::handle_message(client_id, packet, &clients, &stats).await;
            }
        });
        
        // 启动TCP服务器
        let tcp_task = self.start_tcp_server().await?;
        
        // 启动WebSocket服务器（模拟）
        let websocket_task = self.start_websocket_server().await?;
        
        // 启动QUIC服务器（模拟）
        let quic_task = self.start_quic_server().await?;
        
        // 启动统计报告任务
        let stats_task = self.start_stats_reporter().await?;
        
        println!("\n✅ 所有协议服务器启动完成！");
        println!("📊 等待客户端连接...");
        
        // 等待所有任务
        tokio::select! {
            _ = message_handler => println!("消息处理器退出"),
            _ = tcp_task => println!("TCP服务器退出"),
            _ = websocket_task => println!("WebSocket服务器退出"),
            _ = quic_task => println!("QUIC服务器退出"),
            _ = stats_task => println!("统计报告器退出"),
        }
        
        Ok(())
    }
    
    /// 启动TCP服务器
    async fn start_tcp_server(&self) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:9001").await?;
        println!("📡 TCP服务器启动在 127.0.0.1:9001");
        
        let clients = self.clients.clone();
        let next_client_id = self.next_client_id.clone();
        let stats = self.stats.clone();
        let message_tx = self.message_tx.clone();
        
        let task = tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let client_id = {
                    let mut id_gen = next_client_id.lock().await;
                    let id = *id_gen;
                    *id_gen += 1;
                    id
                };
                
                // 添加客户端信息
                {
                    let mut clients_map = clients.lock().await;
                    let mut stats_map = stats.lock().await;
                    
                    clients_map.insert(client_id, ClientInfo {
                        id: client_id,
                        protocol: ProtocolType::Tcp,
                        remote_addr: addr.to_string(),
                        connected_at: SystemTime::now(),
                        packets_received: 0,
                        packets_sent: 0,
                        bytes_received: 0,
                        bytes_sent: 0,
                    });
                    
                    stats_map.active_connections += 1;
                    stats_map.total_connections += 1;
                    stats_map.tcp_connections += 1;
                }
                
                println!("🔗 TCP客户端 {} 连接: {}", client_id, addr);
                
                // 处理客户端连接
                let clients_ref = clients.clone();
                let stats_ref = stats.clone();
                let tx = message_tx.clone();
                
                tokio::spawn(async move {
                    Self::handle_tcp_client(client_id, stream, clients_ref, stats_ref, tx).await;
                });
            }
        });
        
        Ok(task)
    }
    
    /// 启动WebSocket服务器（模拟）
    async fn start_websocket_server(&self) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
        println!("📡 WebSocket服务器启动在 127.0.0.1:9002 (模拟)");
        
        let clients = self.clients.clone();
        let next_client_id = self.next_client_id.clone();
        let stats = self.stats.clone();
        let message_tx = self.message_tx.clone();
        
        let task = tokio::spawn(async move {
            // 模拟WebSocket连接
            for i in 1..=3 {
                let client_id = {
                    let mut id_gen = next_client_id.lock().await;
                    let id = *id_gen;
                    *id_gen += 1;
                    id
                };
                
                // 添加模拟WebSocket客户端
                {
                    let mut clients_map = clients.lock().await;
                    let mut stats_map = stats.lock().await;
                    
                    clients_map.insert(client_id, ClientInfo {
                        id: client_id,
                        protocol: ProtocolType::WebSocket,
                        remote_addr: format!("ws://127.0.0.1:9002/client{}", i),
                        connected_at: SystemTime::now(),
                        packets_received: 0,
                        packets_sent: 0,
                        bytes_received: 0,
                        bytes_sent: 0,
                    });
                    
                    stats_map.active_connections += 1;
                    stats_map.total_connections += 1;
                    stats_map.websocket_connections += 1;
                }
                
                println!("🔗 WebSocket客户端 {} 连接 (模拟)", client_id);
                
                // 模拟发送消息
                let tx = message_tx.clone();
                tokio::spawn(async move {
                    sleep(Duration::from_secs(2)).await;
                    let packet = UnifiedPacket::data(1, format!("WebSocket客户端 {} 的消息", client_id));
                    let _ = tx.send((client_id, packet));
                    
                    // 定期发送心跳
                    let mut interval = tokio::time::interval(Duration::from_secs(30));
                    loop {
                        interval.tick().await;
                        let heartbeat = UnifiedPacket::heartbeat();
                        if tx.send((client_id, heartbeat)).is_err() {
                            break;
                        }
                    }
                });
                
                sleep(Duration::from_millis(500)).await;
            }
            
            // 保持运行
            loop {
                sleep(Duration::from_secs(1)).await;
            }
        });
        
        Ok(task)
    }
    
    /// 启动QUIC服务器（模拟）
    async fn start_quic_server(&self) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
        println!("📡 QUIC服务器启动在 127.0.0.1:9003 (模拟)");
        
        let clients = self.clients.clone();
        let next_client_id = self.next_client_id.clone();
        let stats = self.stats.clone();
        let message_tx = self.message_tx.clone();
        
        let task = tokio::spawn(async move {
            // 模拟QUIC连接
            for i in 1..=2 {
                let client_id = {
                    let mut id_gen = next_client_id.lock().await;
                    let id = *id_gen;
                    *id_gen += 1;
                    id
                };
                
                // 添加模拟QUIC客户端
                {
                    let mut clients_map = clients.lock().await;
                    let mut stats_map = stats.lock().await;
                    
                    clients_map.insert(client_id, ClientInfo {
                        id: client_id,
                        protocol: ProtocolType::Quic,
                        remote_addr: format!("quic://127.0.0.1:9003/client{}", i),
                        connected_at: SystemTime::now(),
                        packets_received: 0,
                        packets_sent: 0,
                        bytes_received: 0,
                        bytes_sent: 0,
                    });
                    
                    stats_map.active_connections += 1;
                    stats_map.total_connections += 1;
                    stats_map.quic_connections += 1;
                }
                
                println!("🔗 QUIC客户端 {} 连接 (模拟)", client_id);
                
                // 模拟发送消息
                let tx = message_tx.clone();
                tokio::spawn(async move {
                    sleep(Duration::from_secs(3)).await;
                    let packet = UnifiedPacket::data(1, format!("QUIC客户端 {} 的高速消息", client_id));
                    let _ = tx.send((client_id, packet));
                    
                    // 定期发送心跳
                    let mut interval = tokio::time::interval(Duration::from_secs(20));
                    loop {
                        interval.tick().await;
                        let heartbeat = UnifiedPacket::heartbeat();
                        if tx.send((client_id, heartbeat)).is_err() {
                            break;
                        }
                    }
                });
                
                sleep(Duration::from_millis(300)).await;
            }
            
            // 保持运行
            loop {
                sleep(Duration::from_secs(1)).await;
            }
        });
        
        Ok(task)
    }
    
    /// 启动统计报告任务
    async fn start_stats_reporter(&self) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
        let stats = self.stats.clone();
        
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                let stats_snapshot = {
                    let stats_lock = stats.lock().await;
                    stats_lock.clone()
                };
                
                println!("\n📊 === 服务器统计报告 ===");
                println!("  活跃连接: {}", stats_snapshot.active_connections);
                println!("  总连接数: {}", stats_snapshot.total_connections);
                println!("  协议分布:");
                println!("    TCP: {}", stats_snapshot.tcp_connections);
                println!("    WebSocket: {}", stats_snapshot.websocket_connections);
                println!("    QUIC: {}", stats_snapshot.quic_connections);
                println!("  总数据包: {}", stats_snapshot.total_packets);
                println!("  总字节数: {:.2} KB", stats_snapshot.total_bytes as f64 / 1024.0);
                println!("========================\n");
            }
        });
        
        Ok(task)
    }
    
    /// 处理TCP客户端连接
    async fn handle_tcp_client(
        client_id: u64,
        mut stream: TcpStream,
        clients: Arc<Mutex<HashMap<u64, ClientInfo>>>,
        stats: Arc<Mutex<ServerStats>>,
        message_tx: mpsc::UnboundedSender<(u64, UnifiedPacket)>,
    ) {
        let mut buffer = vec![0u8; 1024];
        
        loop {
            match stream.read(&mut buffer).await {
                Ok(0) => {
                    // 连接关闭
                    println!("🔌 TCP客户端 {} 断开连接", client_id);
                    break;
                }
                Ok(n) => {
                    // 尝试解析数据包
                    if n >= 9 {
                        match UnifiedPacket::from_bytes(&buffer[..n]) {
                            Ok(packet) => {
                                // 更新统计
                                {
                                    let mut clients_map = clients.lock().await;
                                    let mut stats_map = stats.lock().await;
                                    
                                    if let Some(client) = clients_map.get_mut(&client_id) {
                                        client.packets_received += 1;
                                        client.bytes_received += n as u64;
                                    }
                                    
                                    stats_map.total_packets += 1;
                                    stats_map.total_bytes += n as u64;
                                }
                                
                                // 处理消息并发送回应
                                let response = Self::process_packet_and_create_response(client_id, &packet, &clients, &stats).await;
                                
                                // 发送回应给客户端
                                if let Some(response_packet) = response {
                                    let response_data = response_packet.to_bytes();
                                    if let Err(e) = stream.write_all(&response_data).await {
                                        println!("❌ TCP客户端 {} 发送回应失败: {}", client_id, e);
                                        break;
                                    } else {
                                        // 更新发送统计
                                        {
                                            let mut clients_map = clients.lock().await;
                                            let mut stats_map = stats.lock().await;
                                            
                                            if let Some(client) = clients_map.get_mut(&client_id) {
                                                client.packets_sent += 1;
                                                client.bytes_sent += response_data.len() as u64;
                                            }
                                            
                                            stats_map.total_packets += 1;
                                            stats_map.total_bytes += response_data.len() as u64;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                println!("❌ TCP客户端 {} 数据包解析错误: {}", client_id, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("❌ TCP客户端 {} 读取错误: {}", client_id, e);
                    break;
                }
            }
        }
        
        // 清理客户端信息
        {
            let mut clients_map = clients.lock().await;
            let mut stats_map = stats.lock().await;
            
            clients_map.remove(&client_id);
            stats_map.active_connections = stats_map.active_connections.saturating_sub(1);
            stats_map.tcp_connections = stats_map.tcp_connections.saturating_sub(1);
        }
    }
    
    /// 处理数据包并创建回应
    async fn process_packet_and_create_response(
        client_id: u64,
        packet: &UnifiedPacket,
        clients: &Arc<Mutex<HashMap<u64, ClientInfo>>>,
        stats: &Arc<Mutex<ServerStats>>,
    ) -> Option<UnifiedPacket> {
        let client_info = {
            let clients_map = clients.lock().await;
            clients_map.get(&client_id).cloned()
        };
        
        if let Some(info) = client_info {
            println!("📨 收到来自{}客户端 {} 的消息:", info.protocol, client_id);
            println!("   类型: {:?}, ID: {}", packet.packet_type, packet.message_id);
            
            match packet.packet_type {
                PacketType::Heartbeat => {
                    println!("   内容: 心跳包");
                    println!("📤 发送心跳响应");
                    Some(UnifiedPacket::heartbeat())
                }
                PacketType::Data | PacketType::Control => {
                    if let Some(content) = packet.payload_as_string() {
                        println!("   内容: {}", content);
                        
                        // 创建回显响应
                        let echo_content = format!("回显: {}", content);
                        println!("📤 发送回显响应: {}", echo_content);
                        Some(UnifiedPacket::echo(packet.message_id, echo_content))
                    } else {
                        println!("   内容: 二进制数据 ({} bytes)", packet.payload.len());
                        println!("📤 发送二进制回显响应");
                        Some(UnifiedPacket::echo(packet.message_id, packet.payload.clone()))
                    }
                }
                PacketType::Echo => {
                    if let Some(content) = packet.payload_as_string() {
                        println!("   内容: {}", content);
                    }
                    println!("📤 回显包无需回应");
                    None
                }
                PacketType::Error => {
                    if let Some(content) = packet.payload_as_string() {
                        println!("   错误内容: {}", content);
                    }
                    println!("📤 错误包无需回应");
                    None
                }
                _ => {
                    println!("   未知类型的数据包");
                    None
                }
            }
        } else {
            println!("❌ 客户端 {} 信息未找到", client_id);
            None
        }
    }
    
    /// 处理收到的消息（用于模拟客户端）
    async fn handle_message(
        client_id: u64,
        packet: UnifiedPacket,
        clients: &Arc<Mutex<HashMap<u64, ClientInfo>>>,
        stats: &Arc<Mutex<ServerStats>>,
    ) {
        let client_info = {
            let clients_map = clients.lock().await;
            clients_map.get(&client_id).cloned()
        };
        
        if let Some(info) = client_info {
            println!("📨 收到来自{}客户端 {} 的消息:", info.protocol, client_id);
            println!("   类型: {:?}, ID: {}", packet.packet_type, packet.message_id);
            
            if let Some(content) = packet.payload_as_string() {
                println!("   内容: {}", content);
                
                // 创建回显响应
                let echo_response = UnifiedPacket::echo(packet.message_id, format!("回显: {}", content));
                
                // 模拟客户端不需要真实发送，只打印
                println!("📤 发送回显响应: {}", format!("回显: {}", content));
                
                // 更新发送统计
                {
                    let mut clients_map = clients.lock().await;
                    let mut stats_map = stats.lock().await;
                    
                    if let Some(client) = clients_map.get_mut(&client_id) {
                        client.packets_sent += 1;
                        client.bytes_sent += echo_response.to_bytes().len() as u64;
                    }
                    
                    stats_map.total_packets += 1;
                    stats_map.total_bytes += echo_response.to_bytes().len() as u64;
                }
            } else {
                println!("   内容: 二进制数据 ({} bytes)", packet.payload.len());
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 msgtrans 多协议服务器演示");
    println!("这个演示展示了如何同时支持TCP、WebSocket和QUIC协议");
    println!("提供统一的消息处理和统计功能\n");
    
    let mut server = MultiProtocolServer::new();
    server.start().await?;
    
    Ok(())
} 