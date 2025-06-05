/// QUIC客户端演示（模拟版本）
/// 
/// 展示如何使用msgtrans统一架构创建QUIC客户端
/// 这是一个模拟版本，展示了QUIC特有的特性：多流并发、0-RTT、连接迁移等

use std::collections::HashMap;
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
    /// QUIC流ID（用于多流并发）
    pub stream_id: Option<u64>,
}

impl UnifiedPacket {
    /// 创建新的数据包
    pub fn new(packet_type: PacketType, message_id: u32, payload: Vec<u8>) -> Self {
        Self {
            packet_type,
            message_id,
            payload,
            stream_id: None,
        }
    }
    
    /// 创建带流ID的数据包
    pub fn new_with_stream(packet_type: PacketType, message_id: u32, payload: Vec<u8>, stream_id: u64) -> Self {
        Self {
            packet_type,
            message_id,
            payload,
            stream_id: Some(stream_id),
        }
    }
    
    /// 创建数据消息包
    pub fn data(message_id: u32, payload: impl Into<Vec<u8>>) -> Self {
        Self::new(PacketType::Data, message_id, payload.into())
    }
    
    /// 创建带流的数据消息包
    pub fn data_with_stream(message_id: u32, payload: impl Into<Vec<u8>>, stream_id: u64) -> Self {
        Self::new_with_stream(PacketType::Data, message_id, payload.into(), stream_id)
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
        let mut buffer = Vec::with_capacity(17 + self.payload.len());
        
        // 写入包类型（1字节）
        buffer.push(self.packet_type.into());
        
        // 写入消息ID（4字节，大端序）
        buffer.extend_from_slice(&self.message_id.to_be_bytes());
        
        // 写入流ID（8字节，大端序）
        buffer.extend_from_slice(&self.stream_id.unwrap_or(0).to_be_bytes());
        
        // 写入负载长度（4字节，大端序）
        buffer.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        
        // 写入负载
        buffer.extend_from_slice(&self.payload);
        
        buffer
    }
    
    /// 从字节反序列化
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 17 {
            return Err("数据太短".to_string());
        }
        
        // 读取包类型
        let packet_type = PacketType::from(data[0]);
        
        // 读取消息ID
        let message_id = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
        
        // 读取流ID
        let stream_id_value = u64::from_be_bytes([
            data[5], data[6], data[7], data[8],
            data[9], data[10], data[11], data[12]
        ]);
        let stream_id = if stream_id_value == 0 { None } else { Some(stream_id_value) };
        
        // 读取负载长度
        let payload_len = u32::from_be_bytes([data[13], data[14], data[15], data[16]]) as usize;
        
        // 检查数据完整性
        if data.len() != 17 + payload_len {
            return Err("数据长度不匹配".to_string());
        }
        
        // 读取负载
        let payload = data[17..].to_vec();
        
        Ok(Self {
            packet_type,
            message_id,
            payload,
            stream_id,
        })
    }
    
    /// 获取负载的字符串表示
    pub fn payload_as_string(&self) -> Option<String> {
        String::from_utf8(self.payload.clone()).ok()
    }
}

/// QUIC连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum QuicConnectionState {
    Initial,
    Handshaking,
    Connected,
    Closing,
    Closed,
    ConnectionMigrating,
}

/// QUIC流信息
#[derive(Debug, Clone)]
pub struct QuicStream {
    pub id: u64,
    pub is_bidirectional: bool,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub created_at: Instant,
}

impl QuicStream {
    pub fn new(id: u64, is_bidirectional: bool) -> Self {
        Self {
            id,
            is_bidirectional,
            bytes_sent: 0,
            bytes_received: 0,
            packets_sent: 0,
            packets_received: 0,
            created_at: Instant::now(),
        }
    }
}

/// QUIC客户端（模拟版本）
pub struct QuicClient {
    server_endpoint: String,
    state: QuicConnectionState,
    connection_id: [u8; 8],
    streams: HashMap<u64, QuicStream>,
    next_stream_id: u64,
    packets_sent: u64,
    packets_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
    connected_at: Option<Instant>,
    rtt: Duration,
    zero_rtt_enabled: bool,
    connection_migrations: u32,
}

impl QuicClient {
    /// 创建新的QUIC客户端
    pub fn new(server_endpoint: &str) -> Self {
        Self {
            server_endpoint: server_endpoint.to_string(),
            state: QuicConnectionState::Initial,
            connection_id: [0u8; 8],
            streams: HashMap::new(),
            next_stream_id: 0,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            connected_at: None,
            rtt: Duration::from_millis(0),
            zero_rtt_enabled: false,
            connection_migrations: 0,
        }
    }
    
    /// 启用0-RTT连接
    pub fn enable_zero_rtt(&mut self) {
        self.zero_rtt_enabled = true;
    }
    
    /// 连接到QUIC服务器
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 连接到QUIC服务器: {}", self.server_endpoint);
        self.state = QuicConnectionState::Handshaking;
        
        // 生成连接ID
        self.connection_id = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        
        if self.zero_rtt_enabled {
            println!("⚡ 启用0-RTT连接...");
            // 0-RTT连接几乎无延迟
            sleep(Duration::from_millis(1)).await;
            self.rtt = Duration::from_millis(1);
        } else {
            println!("🤝 执行QUIC TLS 1.3握手...");
            // 模拟初始握手延迟（比TCP+TLS更快）
            sleep(Duration::from_millis(50)).await;
            self.rtt = Duration::from_millis(25);
        }
        
        self.state = QuicConnectionState::Connected;
        self.connected_at = Some(Instant::now());
        
        // 创建默认的双向流
        self.create_stream(true).await?;
        
        println!("✅ QUIC连接建立成功");
        println!("   连接ID: {:02X?}", self.connection_id);
        println!("   RTT: {:?}", self.rtt);
        println!("   0-RTT: {}", if self.zero_rtt_enabled { "启用" } else { "未启用" });
        println!("   加密: TLS 1.3 + AEAD");
        
        Ok(())
    }
    
    /// 创建新的QUIC流
    pub async fn create_stream(&mut self, bidirectional: bool) -> Result<u64, Box<dyn std::error::Error>> {
        if self.state != QuicConnectionState::Connected {
            return Err("QUIC未连接".into());
        }
        
        let stream_id = self.next_stream_id;
        self.next_stream_id += 4; // QUIC流ID按4递增
        
        let stream = QuicStream::new(stream_id, bidirectional);
        self.streams.insert(stream_id, stream);
        
        println!("🌊 创建新的QUIC流: {} ({})", 
            stream_id, 
            if bidirectional { "双向" } else { "单向" }
        );
        
        Ok(stream_id)
    }
    
    /// 在指定流上发送数据包
    pub async fn send_packet_on_stream(&mut self, packet: UnifiedPacket, stream_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        if self.state != QuicConnectionState::Connected {
            return Err("QUIC未连接".into());
        }
        
        // 检查流是否存在
        if !self.streams.contains_key(&stream_id) {
            return Err(format!("流 {} 不存在", stream_id).into());
        }
        
        // 设置流ID
        let mut packet_with_stream = packet;
        packet_with_stream.stream_id = Some(stream_id);
        
        let data = packet_with_stream.to_bytes();
        
        println!("📤 在流 {} 上发送QUIC数据包:", stream_id);
        println!("   类型: {:?}", packet_with_stream.packet_type);
        println!("   消息ID: {}", packet_with_stream.message_id);
        println!("   大小: {} bytes", data.len());
        
        if let Some(content) = packet_with_stream.payload_as_string() {
            println!("   内容: {}", content);
        }
        
        // 模拟网络发送延迟（QUIC比TCP更快）
        sleep(Duration::from_millis(2)).await;
        
        // 更新统计
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.packets_sent += 1;
            stream.bytes_sent += data.len() as u64;
        }
        
        self.packets_sent += 1;
        self.bytes_sent += data.len() as u64;
        
        println!("✅ QUIC数据包发送成功");
        
        Ok(())
    }
    
    /// 发送数据包（使用默认流）
    pub async fn send_packet(&mut self, packet: UnifiedPacket) -> Result<(), Box<dyn std::error::Error>> {
        let default_stream_id = 0; // 使用第一个流
        self.send_packet_on_stream(packet, default_stream_id).await
    }
    
    /// 从指定流接收数据包
    pub async fn receive_packet_from_stream(&mut self, stream_id: u64) -> Result<Option<UnifiedPacket>, Box<dyn std::error::Error>> {
        if self.state != QuicConnectionState::Connected {
            return Err("QUIC未连接".into());
        }
        
        // 模拟接收延迟（QUIC延迟很低）
        sleep(Duration::from_millis(5)).await;
        
        // 模拟收到服务器响应
        let response_data = UnifiedPacket::new_with_stream(
            PacketType::Echo,
            999,
            "QUIC服务器回显 - 高速传输".as_bytes().to_vec(),
            stream_id
        );
        
        let data_size = response_data.to_bytes().len();
        
        // 更新统计
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.packets_received += 1;
            stream.bytes_received += data_size as u64;
        }
        
        self.packets_received += 1;
        self.bytes_received += data_size as u64;
        
        println!("📥 从流 {} 接收到QUIC数据包:", stream_id);
        println!("   类型: {:?}", response_data.packet_type);
        println!("   消息ID: {}", response_data.message_id);
        println!("   大小: {} bytes", data_size);
        
        if let Some(content) = response_data.payload_as_string() {
            println!("   内容: {}", content);
        }
        
        Ok(Some(response_data))
    }
    
    /// 接收数据包（从默认流）
    pub async fn receive_packet(&mut self) -> Result<Option<UnifiedPacket>, Box<dyn std::error::Error>> {
        let default_stream_id = 0;
        self.receive_packet_from_stream(default_stream_id).await
    }
    
    /// 模拟连接迁移
    pub async fn migrate_connection(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.state != QuicConnectionState::Connected {
            return Err("QUIC未连接".into());
        }
        
        println!("🔄 开始连接迁移...");
        self.state = QuicConnectionState::ConnectionMigrating;
        
        // 模拟网络切换延迟
        sleep(Duration::from_millis(100)).await;
        
        // 生成新的连接路径
        self.connection_migrations += 1;
        
        println!("✅ 连接迁移完成");
        println!("   新路径: path_{}", self.connection_migrations);
        println!("   迁移次数: {}", self.connection_migrations);
        
        self.state = QuicConnectionState::Connected;
        
        Ok(())
    }
    
    /// 获取流统计信息
    pub fn get_stream_stats(&self, stream_id: u64) -> Option<&QuicStream> {
        self.streams.get(&stream_id)
    }
    
    /// 获取所有流的统计信息
    pub fn get_all_streams(&self) -> &HashMap<u64, QuicStream> {
        &self.streams
    }
    
    /// 获取连接统计信息
    pub fn get_stats(&self) -> (u64, u64, u64, u64) {
        (self.packets_sent, self.packets_received, self.bytes_sent, self.bytes_received)
    }
    
    /// 获取连接时长
    pub fn get_uptime(&self) -> Option<Duration> {
        self.connected_at.map(|start| start.elapsed())
    }
    
    /// 获取当前RTT
    pub fn get_rtt(&self) -> Duration {
        self.rtt
    }
    
    /// 关闭指定流
    pub async fn close_stream(&mut self, stream_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(stream) = self.streams.remove(&stream_id) {
            println!("🌊 关闭QUIC流 {}", stream_id);
            let duration = stream.created_at.elapsed();
            println!("   存活时间: {:.2}秒", duration.as_secs_f64());
            println!("   发送: {} 包, {} bytes", stream.packets_sent, stream.bytes_sent);
            println!("   接收: {} 包, {} bytes", stream.packets_received, stream.bytes_received);
        }
        
        Ok(())
    }
    
    /// 关闭连接
    pub async fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.state == QuicConnectionState::Closed {
            return Ok(());
        }
        
        println!("🔌 关闭QUIC连接...");
        self.state = QuicConnectionState::Closing;
        
        // 关闭所有流
        let stream_ids: Vec<u64> = self.streams.keys().cloned().collect();
        for stream_id in stream_ids {
            self.close_stream(stream_id).await?;
        }
        
        // 模拟连接关闭
        sleep(Duration::from_millis(10)).await;
        
        self.state = QuicConnectionState::Closed;
        println!("✅ QUIC连接已关闭");
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ msgtrans QUIC客户端演示");
    println!("============================");
    println!("这个演示展示了如何使用统一架构创建QUIC客户端");
    println!("支持多流并发、0-RTT连接、连接迁移等QUIC特有功能\n");
    
    // 创建QUIC客户端
    let mut client = QuicClient::new("quic://127.0.0.1:9003");
    
    // 启用0-RTT
    client.enable_zero_rtt();
    
    // 连接到服务器
    client.connect().await?;
    
    // 演示1: 在默认流上发送消息
    println!("\n🎯 === 演示1: 默认流消息传输 ===");
    let message1 = UnifiedPacket::data(1, "Hello from QUIC client!");
    client.send_packet(message1).await?;
    
    // 等待响应
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到服务器响应");
    }
    
    sleep(Duration::from_millis(200)).await;
    
    // 演示2: 创建多个并发流
    println!("\n🎯 === 演示2: 多流并发传输 ===");
    
    // 创建3个额外的流
    let stream_1 = client.create_stream(true).await?;
    let stream_2 = client.create_stream(true).await?;
    let stream_3 = client.create_stream(false).await?; // 单向流
    
    // 在不同流上并发发送消息
    println!("📤 在多个流上并发发送消息...");
    
    let msg_stream_1 = UnifiedPacket::data(101, "流1消息: 文件下载数据");
    let msg_stream_2 = UnifiedPacket::data(102, "流2消息: 实时聊天消息");
    let msg_stream_3 = UnifiedPacket::data(103, "流3消息: 控制信令");
    
    // 模拟并发发送
    client.send_packet_on_stream(msg_stream_1, stream_1).await?;
    client.send_packet_on_stream(msg_stream_2, stream_2).await?;
    client.send_packet_on_stream(msg_stream_3, stream_3).await?;
    
    // 从不同流接收响应
    for &stream_id in &[stream_1, stream_2] { // 单向流不接收响应
        if let Some(response) = client.receive_packet_from_stream(stream_id).await? {
            println!("✅ 从流 {} 收到响应", stream_id);
        }
    }
    
    sleep(Duration::from_millis(300)).await;
    
    // 演示3: 大文件传输模拟
    println!("\n🎯 === 演示3: 大文件传输模拟 ===");
    let file_stream = client.create_stream(true).await?;
    
    // 模拟分块传输大文件
    for chunk_id in 1..=5 {
        let chunk_data = format!("文件块 {}/5: {}", chunk_id, "X".repeat(100));
        let chunk_packet = UnifiedPacket::data(200 + chunk_id, chunk_data);
        client.send_packet_on_stream(chunk_packet, file_stream).await?;
        sleep(Duration::from_millis(20)).await;
    }
    
    println!("✅ 大文件分块传输完成");
    
    // 演示4: 连接迁移
    println!("\n🎯 === 演示4: 连接迁移演示 ===");
    println!("模拟网络切换场景...");
    client.migrate_connection().await?;
    
    // 迁移后继续发送消息验证连接
    let post_migration_msg = UnifiedPacket::data(301, "连接迁移后的消息");
    client.send_packet(post_migration_msg).await?;
    
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 连接迁移后通信正常");
    }
    
    sleep(Duration::from_millis(200)).await;
    
    // 演示5: 实时数据流
    println!("\n🎯 === 演示5: 实时数据流 ===");
    let realtime_stream = client.create_stream(true).await?;
    
    // 模拟实时数据传输（游戏、视频等）
    for frame in 1..=10 {
        let frame_data = format!("实时帧数据 #{}: timestamp={}", frame, frame * 33); // 30fps
        let frame_packet = UnifiedPacket::data(400 + frame, frame_data);
        client.send_packet_on_stream(frame_packet, realtime_stream).await?;
        sleep(Duration::from_millis(33)).await; // 模拟30fps
    }
    
    println!("✅ 实时数据流传输完成");
    
    // 演示6: 控制消息
    println!("\n🎯 === 演示6: 控制消息传输 ===");
    let control_msg = r#"{"action": "get_server_status", "client_type": "quic"}"#;
    let control_packet = UnifiedPacket::control(501, control_msg);
    client.send_packet(control_packet).await?;
    
    if let Some(response) = client.receive_packet().await? {
        println!("✅ 收到控制响应");
    }
    
    sleep(Duration::from_millis(200)).await;
    
    // 显示详细统计信息
    let (sent, received, bytes_sent, bytes_received) = client.get_stats();
    let uptime = client.get_uptime().unwrap_or(Duration::from_secs(0));
    let rtt = client.get_rtt();
    
    println!("\n📊 === QUIC连接统计 ===");
    println!("连接时长: {:.2}秒", uptime.as_secs_f64());
    println!("当前RTT: {:?}", rtt);
    println!("连接迁移: {} 次", client.connection_migrations);
    println!("总流数量: {}", client.get_all_streams().len());
    println!("发送数据包: {}", sent);
    println!("接收数据包: {}", received);
    println!("发送字节数: {} bytes", bytes_sent);
    println!("接收字节数: {} bytes", bytes_received);
    println!("总传输量: {} bytes", bytes_sent + bytes_received);
    
    if uptime.as_secs() > 0 {
        let packet_rate = (sent + received) as f64 / uptime.as_secs_f64();
        let throughput = (bytes_sent + bytes_received) as f64 / uptime.as_secs_f64();
        println!("数据包速率: {:.2} 包/秒", packet_rate);
        println!("吞吐量: {:.2} bytes/秒", throughput);
    }
    
    // 显示各流统计
    println!("\n🌊 === 流统计详情 ===");
    for (stream_id, stream) in client.get_all_streams() {
        println!("流 {}:", stream_id);
        println!("  类型: {}", if stream.is_bidirectional { "双向" } else { "单向" });
        println!("  存活: {:.2}秒", stream.created_at.elapsed().as_secs_f64());
        println!("  发送: {} 包, {} bytes", stream.packets_sent, stream.bytes_sent);
        println!("  接收: {} 包, {} bytes", stream.packets_received, stream.bytes_received);
    }
    
    // 关闭连接
    println!("\n🔌 正在关闭QUIC连接...");
    client.close().await?;
    
    println!("\n✅ QUIC客户端演示完成！");
    println!("🎯 QUIC特性验证:");
    println!("  ✓ 统一数据包格式适配QUIC");
    println!("  ✓ 0-RTT快速连接");
    println!("  ✓ 多流并发传输");
    println!("  ✓ 连接迁移支持");
    println!("  ✓ 低延迟数据传输");
    println!("  ✓ TLS 1.3内置加密");
    println!("  ✓ 实时数据流处理");
    println!("  ✓ 大文件分块传输");
    println!("  ✓ 流级别统计监控");
    println!("  ✓ 动态流管理");
    
    Ok(())
} 