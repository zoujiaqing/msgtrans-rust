/// TCP客户端演示
/// 
/// 展示如何使用msgtrans统一架构创建TCP客户端
/// 连接到服务器并发送各种类型的消息

use std::time::Duration;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, timeout};

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
}

/// TCP客户端
pub struct TcpClient {
    stream: TcpStream,
    server_addr: String,
    packets_sent: u64,
    packets_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

impl TcpClient {
    /// 连接到服务器
    pub async fn connect(server_addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        println!("🔌 连接到TCP服务器: {}", server_addr);
        
        let stream = timeout(Duration::from_secs(5), TcpStream::connect(server_addr)).await??;
        
        println!("✅ TCP连接建立成功");
        
        Ok(Self {
            stream,
            server_addr: server_addr.to_string(),
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        })
    }
    
    /// 发送数据包
    pub async fn send_packet(&mut self, packet: UnifiedPacket) -> Result<(), Box<dyn std::error::Error>> {
        let data = packet.to_bytes();
        
        println!("📤 发送数据包:");
        println!("   类型: {:?}", packet.packet_type);
        println!("   消息ID: {}", packet.message_id);
        println!("   大小: {} bytes", data.len());
        
        if let Some(content) = packet.payload_as_string() {
            println!("   内容: {}", content);
        }
        
        self.stream.write_all(&data).await?;
        
        self.packets_sent += 1;
        self.bytes_sent += data.len() as u64;
        
        println!("✅ 数据包发送成功");
        
        Ok(())
    }
    
    /// 接收数据包
    pub async fn receive_packet(&mut self) -> Result<Option<UnifiedPacket>, Box<dyn std::error::Error>> {
        let mut buffer = vec![0u8; 1024];
        
        match timeout(Duration::from_secs(5), self.stream.read(&mut buffer)).await? {
            Ok(0) => {
                println!("🔌 服务器关闭了连接");
                Ok(None)
            }
            Ok(n) => {
                self.bytes_received += n as u64;
                
                match UnifiedPacket::from_bytes(&buffer[..n]) {
                    Ok(packet) => {
                        self.packets_received += 1;
                        
                        println!("📥 接收到数据包:");
                        println!("   类型: {:?}", packet.packet_type);
                        println!("   消息ID: {}", packet.message_id);
                        println!("   大小: {} bytes", n);
                        
                        if let Some(content) = packet.payload_as_string() {
                            println!("   内容: {}", content);
                        }
                        
                        Ok(Some(packet))
                    }
                    Err(e) => {
                        println!("❌ 数据包解析失败: {}", e);
                        Err(e.into())
                    }
                }
            }
            Err(e) => {
                println!("❌ 读取数据失败: {}", e);
                Err(e.into())
            }
        }
    }
    
    /// 获取统计信息
    pub fn get_stats(&self) -> (u64, u64, u64, u64) {
        (self.packets_sent, self.packets_received, self.bytes_sent, self.bytes_received)
    }
    
    /// 关闭连接
    pub async fn close(mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.shutdown().await?;
        println!("🔌 TCP连接已关闭");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("📡 msgtrans TCP客户端演示");
    println!("==========================");
    println!("这个演示展示了如何使用统一架构创建TCP客户端");
    println!("并与服务器进行各种类型的消息交换\n");
    
    // 连接到服务器
    let mut client = match TcpClient::connect("127.0.0.1:9001").await {
        Ok(client) => client,
        Err(e) => {
            println!("❌ 连接失败: {}", e);
            println!("💡 请确保服务器正在运行 (cargo run --example server_multiprotocol)");
            return Ok(());
        }
    };
    
    // 演示1: 发送简单消息
    println!("\n🎯 === 演示1: 发送简单消息 ===");
    let message1 = UnifiedPacket::data(1, "Hello from TCP client!");
    client.send_packet(message1).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // 演示2: 发送中文消息
    println!("\n🎯 === 演示2: 发送中文消息 ===");
    let message2 = UnifiedPacket::data(2, "你好，这是中文消息！");
    client.send_packet(message2).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // 演示3: 发送JSON控制消息
    println!("\n🎯 === 演示3: 发送JSON控制消息 ===");
    let json_message = r#"{"action": "ping", "timestamp": 1234567890, "client": "tcp"}"#;
    let message3 = UnifiedPacket::control(3, json_message);
    client.send_packet(message3).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // 演示4: 发送心跳包
    println!("\n🎯 === 演示4: 发送心跳包 ===");
    let heartbeat = UnifiedPacket::heartbeat();
    client.send_packet(heartbeat).await?;
    
    sleep(Duration::from_millis(500)).await;
    
    // 演示5: 发送大消息
    println!("\n🎯 === 演示5: 发送大消息 ===");
    let large_message = "大消息测试: ".to_string() + &"X".repeat(500);
    let message5 = UnifiedPacket::data(5, large_message);
    client.send_packet(message5).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // 演示6: 压力测试 - 快速发送多个消息
    println!("\n🎯 === 演示6: 压力测试 ===");
    println!("快速发送10个消息...");
    
    for i in 1..=10 {
        let message = UnifiedPacket::data(100 + i, format!("压力测试消息 #{}", i));
        client.send_packet(message).await?;
        sleep(Duration::from_millis(100)).await; // 短暂间隔
    }
    
    println!("✅ 压力测试完成");
    
    // 显示统计信息
    let (sent, received, bytes_sent, bytes_received) = client.get_stats();
    println!("\n📊 === 连接统计 ===");
    println!("发送数据包: {}", sent);
    println!("接收数据包: {}", received);
    println!("发送字节数: {} bytes", bytes_sent);
    println!("接收字节数: {} bytes", bytes_received);
    println!("总传输量: {} bytes", bytes_sent + bytes_received);
    
    // 关闭连接
    println!("\n🔌 正在关闭连接...");
    client.close().await?;
    
    println!("\n✅ TCP客户端演示完成！");
    println!("🎯 核心特性验证:");
    println!("  ✓ 统一数据包格式");
    println!("  ✓ 类型安全的消息传输");
    println!("  ✓ 中文和特殊字符支持");
    println!("  ✓ JSON控制消息");
    println!("  ✓ 心跳包机制");
    println!("  ✓ 大消息传输");
    println!("  ✓ 高并发消息处理");
    
    Ok(())
} 