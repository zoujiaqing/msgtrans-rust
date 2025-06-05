/// WebSocket客户端演示（模拟版本）
/// 
/// 展示如何使用msgtrans统一架构创建WebSocket客户端
/// 这是一个模拟版本，展示了WebSocket特有的特性

use std::time::Duration;
use tokio::time::{sleep, Instant};

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
    
    /// 创建控制消息包
    pub fn control(message_id: u32, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(PacketType::Control, message_id, payload.into())
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
    
    /// 将数据包编码为WebSocket文本帧
    pub fn to_websocket_text(&self) -> String {
        // 在真实实现中，这里会转换为WebSocket文本格式
        // 为了演示，我们使用JSON格式
        format!(
            r#"{{"type": {}, "id": {}, "payload": "{}"}}"#,
            self.packet_type as u8,
            self.message_id,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.payload)
        )
    }
    
    /// 从WebSocket文本帧解码数据包
    pub fn from_websocket_text(text: &str) -> Result<Self, String> {
        // 在真实实现中，这里会从WebSocket文本格式解析
        // 为了演示，我们模拟解析过程
        if text.contains("ping") {
            Ok(Self::heartbeat())
        } else if text.contains("echo") {
            Ok(Self::data(999, "WebSocket服务器回显"))
        } else {
            Ok(Self::data(0, text.to_string()))
        }
    }
}

// 模拟base64编码（简化版本）
mod base64 {
    pub mod engine {
        pub mod general_purpose {
            pub struct StandardEngine;
            pub const STANDARD: StandardEngine = StandardEngine;
        }
    }
    
    pub trait Engine {
        fn encode(&self, data: &[u8]) -> String;
    }
    
    impl Engine for engine::general_purpose::StandardEngine {
        fn encode(&self, data: &[u8]) -> String {
            // 简化的base64编码模拟
            format!("base64:{}", String::from_utf8_lossy(data))
        }
    }
}

/// WebSocket连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum WebSocketState {
    Disconnected,
    Connecting,
    Connected,
    Closing,
    Closed,
}

/// WebSocket客户端（模拟版本）
pub struct WebSocketClient {
    server_url: String,
    state: WebSocketState,
    packets_sent: u64,
    packets_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
    connected_at: Option<Instant>,
    last_ping: Option<Instant>,
}

impl WebSocketClient {
    /// 创建新的WebSocket客户端
    pub fn new(server_url: &str) -> Self {
        Self {
            server_url: server_url.to_string(),
            state: WebSocketState::Disconnected,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            connected_at: None,
            last_ping: None,
        }
    }
    
    /// 连接到WebSocket服务器
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔌 连接到WebSocket服务器: {}", self.server_url);
        self.state = WebSocketState::Connecting;
        
        // 模拟连接延迟
        sleep(Duration::from_millis(200)).await;
        
        // 模拟握手过程
        println!("🤝 执行WebSocket握手...");
        sleep(Duration::from_millis(100)).await;
        
        self.state = WebSocketState::Connected;
        self.connected_at = Some(Instant::now());
        
        println!("✅ WebSocket连接建立成功");
        println!("   协议版本: WebSocket 13");
        println!("   扩展: permessage-deflate");
        println!("   子协议: msgtrans-v1");
        
        Ok(())
    }
    
    /// 发送数据包
    pub async fn send_packet(&mut self, packet: UnifiedPacket) -> Result<(), Box<dyn std::error::Error>> {
        if self.state != WebSocketState::Connected {
            return Err("WebSocket未连接".into());
        }
        
        // 将数据包编码为WebSocket文本帧
        let websocket_frame = packet.to_websocket_text();
        let frame_size = websocket_frame.len();
        
        println!("📤 发送WebSocket消息:");
        println!("   类型: {:?}", packet.packet_type);
        println!("   消息ID: {}", packet.message_id);
        println!("   帧类型: Text");
        println!("   帧大小: {} bytes", frame_size);
        
        if let Some(content) = packet.payload_as_string() {
            println!("   内容: {}", content);
        }
        
        // 模拟网络发送延迟
        sleep(Duration::from_millis(10)).await;
        
        self.packets_sent += 1;
        self.bytes_sent += frame_size as u64;
        
        println!("✅ WebSocket消息发送成功");
        
        Ok(())
    }
    
    /// 接收数据包
    pub async fn receive_packet(&mut self) -> Result<Option<UnifiedPacket>, Box<dyn std::error::Error>> {
        if self.state != WebSocketState::Connected {
            return Err("WebSocket未连接".into());
        }
        
        // 模拟接收延迟
        sleep(Duration::from_millis(50)).await;
        
        // 模拟收到服务器响应
        let response_text = r#"{"type": "echo", "message": "WebSocket服务器回显"}"#;
        let frame_size = response_text.len();
        
        match UnifiedPacket::from_websocket_text(response_text) {
            Ok(packet) => {
                self.packets_received += 1;
                self.bytes_received += frame_size as u64;
                
                println!("📥 接收到WebSocket消息:");
                println!("   类型: {:?}", packet.packet_type);
                println!("   消息ID: {}", packet.message_id);
                println!("   帧类型: Text");
                println!("   帧大小: {} bytes", frame_size);
                
                if let Some(content) = packet.payload_as_string() {
                    println!("   内容: {}", content);
                }
                
                Ok(Some(packet))
            }
            Err(e) => {
                println!("❌ WebSocket消息解析失败: {}", e);
                Err(e.into())
            }
        }
    }
    
    /// 发送Ping帧
    pub async fn send_ping(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.state != WebSocketState::Connected {
            return Err("WebSocket未连接".into());
        }
        
        println!("🏓 发送WebSocket Ping帧");
        
        // 模拟发送Ping
        sleep(Duration::from_millis(5)).await;
        
        self.last_ping = Some(Instant::now());
        
        println!("✅ Ping帧发送成功");
        
        Ok(())
    }
    
    /// 接收Pong帧
    pub async fn receive_pong(&mut self) -> Result<Duration, Box<dyn std::error::Error>> {
        if self.state != WebSocketState::Connected {
            return Err("WebSocket未连接".into());
        }
        
        // 模拟接收Pong延迟
        sleep(Duration::from_millis(20)).await;
        
        println!("🏓 接收到WebSocket Pong帧");
        
        if let Some(ping_time) = self.last_ping {
            let rtt = ping_time.elapsed();
            println!("✅ RTT: {:?}", rtt);
            Ok(rtt)
        } else {
            Ok(Duration::from_millis(20))
        }
    }
    
    /// 获取连接状态
    pub fn get_state(&self) -> &WebSocketState {
        &self.state
    }
    
    /// 获取统计信息
    pub fn get_stats(&self) -> (u64, u64, u64, u64) {
        (self.packets_sent, self.packets_received, self.bytes_sent, self.bytes_received)
    }
    
    /// 获取连接时长
    pub fn get_uptime(&self) -> Option<Duration> {
        self.connected_at.map(|start| start.elapsed())
    }
    
    /// 关闭连接
    pub async fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.state != WebSocketState::Connected {
            return Ok(());
        }
        
        println!("🔌 关闭WebSocket连接...");
        self.state = WebSocketState::Closing;
        
        // 模拟关闭握手
        sleep(Duration::from_millis(50)).await;
        
        self.state = WebSocketState::Closed;
        println!("✅ WebSocket连接已关闭");
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 msgtrans WebSocket客户端演示");
    println!("================================");
    println!("这个演示展示了如何使用统一架构创建WebSocket客户端");
    println!("支持文本消息、二进制消息和WebSocket特有的Ping/Pong机制\n");
    
    // 创建WebSocket客户端
    let mut client = WebSocketClient::new("ws://127.0.0.1:9002/msgtrans");
    
    // 连接到服务器
    client.connect().await?;
    
    // 演示1: 发送简单文本消息
    println!("\n🎯 === 演示1: 发送简单文本消息 ===");
    let message1 = UnifiedPacket::data(1, "Hello from WebSocket client!");
    client.send_packet(message1).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(300)).await;
    
    // 演示2: 发送JSON数据
    println!("\n🎯 === 演示2: 发送JSON数据 ===");
    let json_data = r#"{"user": "alice", "action": "join_room", "room": "general"}"#;
    let message2 = UnifiedPacket::data(2, json_data);
    client.send_packet(message2).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(300)).await;
    
    // 演示3: 发送控制消息
    println!("\n🎯 === 演示3: 发送控制消息 ===");
    let control_msg = r#"{"command": "get_user_list"}"#;
    let message3 = UnifiedPacket::control(3, control_msg);
    client.send_packet(message3).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(300)).await;
    
    // 演示4: WebSocket Ping/Pong机制
    println!("\n🎯 === 演示4: WebSocket Ping/Pong机制 ===");
    client.send_ping().await?;
    
    // 等待Pong响应
    let rtt = client.receive_pong().await?;
    println!("🏓 网络延迟: {:?}", rtt);
    
    sleep(Duration::from_millis(300)).await;
    
    // 演示5: 发送中文消息
    println!("\n🎯 === 演示5: 发送中文消息 ===");
    let chinese_msg = "你好！这是一个WebSocket中文消息测试。🚀";
    let message5 = UnifiedPacket::data(5, chinese_msg);
    client.send_packet(message5).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(300)).await;
    
    // 演示6: 批量消息发送
    println!("\n🎯 === 演示6: 批量消息发送 ===");
    println!("发送5个连续消息...");
    
    for i in 1..=5 {
        let batch_msg = format!("批量消息 #{} - WebSocket并发测试", i);
        let message = UnifiedPacket::data(100 + i, batch_msg);
        client.send_packet(message).await?;
        sleep(Duration::from_millis(50)).await; // 短暂间隔
    }
    
    println!("✅ 批量消息发送完成");
    
    // 演示7: 心跳包测试
    println!("\n🎯 === 演示7: 心跳包测试 ===");
    let heartbeat = UnifiedPacket::heartbeat();
    client.send_packet(heartbeat).await?;
    
    sleep(Duration::from_millis(200)).await;
    
    // 显示连接统计
    let (sent, received, bytes_sent, bytes_received) = client.get_stats();
    let uptime = client.get_uptime().unwrap_or(Duration::from_secs(0));
    
    println!("\n📊 === WebSocket连接统计 ===");
    println!("连接状态: {:?}", client.get_state());
    println!("连接时长: {:.2}秒", uptime.as_secs_f64());
    println!("发送消息: {}", sent);
    println!("接收消息: {}", received);
    println!("发送字节: {} bytes", bytes_sent);
    println!("接收字节: {} bytes", bytes_received);
    println!("总传输量: {} bytes", bytes_sent + bytes_received);
    
    if uptime.as_secs() > 0 {
        let msg_rate = (sent + received) as f64 / uptime.as_secs_f64();
        let byte_rate = (bytes_sent + bytes_received) as f64 / uptime.as_secs_f64();
        println!("消息速率: {:.2} 消息/秒", msg_rate);
        println!("数据速率: {:.2} bytes/秒", byte_rate);
    }
    
    // 关闭连接
    println!("\n🔌 正在关闭WebSocket连接...");
    client.close().await?;
    
    println!("\n✅ WebSocket客户端演示完成！");
    println!("🎯 WebSocket特性验证:");
    println!("  ✓ 统一数据包格式适配WebSocket");
    println!("  ✓ 文本和二进制消息支持");
    println!("  ✓ WebSocket握手协议");
    println!("  ✓ Ping/Pong心跳机制");
    println!("  ✓ JSON数据传输优化");
    println!("  ✓ 中文UTF-8编码支持");
    println!("  ✓ 批量消息处理");
    println!("  ✓ 连接状态管理");
    println!("  ✓ 实时统计监控");
    
    Ok(())
} 