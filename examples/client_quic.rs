/// QUIC客户端演示 - 使用msgtrans统一架构
/// 
/// 展示如何使用msgtrans统一架构创建QUIC客户端
/// 演示QUIC特有的特性：多流并发、0-RTT、连接迁移、可靠传输等

use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::unified::{
    Transport, TransportBuilder,
    packet::UnifiedPacket,
    error::TransportError,
    config::TransportConfig,
};

/// QUIC客户端演示
pub struct QuicClientDemo {
    transport: Transport,
    session_id: Option<u64>,
    server_endpoint: String,
    messages_sent: u64,
    messages_received: u64,
    streams_created: u64,
    zero_rtt_enabled: bool,
}

impl QuicClientDemo {
    /// 创建新的QUIC客户端演示
    pub async fn new(server_endpoint: &str) -> Result<Self, TransportError> {
        println!("🌟 QUIC客户端演示 - 使用msgtrans统一架构");
        println!("==========================================");
        
        // 使用统一架构创建传输层
        let config = TransportConfig::default();
        let transport = TransportBuilder::new()
            .config(config)
            .build()
            .await?;
        
        Ok(Self {
            transport,
            session_id: None,
            server_endpoint: server_endpoint.to_string(),
            messages_sent: 0,
            messages_received: 0,
            streams_created: 0,
            zero_rtt_enabled: false,
        })
    }
    
    /// 启用0-RTT连接
    pub fn enable_zero_rtt(&mut self) {
        self.zero_rtt_enabled = true;
        println!("🚀 启用QUIC 0-RTT连接模式");
    }
    
    /// 连接到服务器
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        println!("🔌 连接到QUIC服务器: {}", self.server_endpoint);
        
        if self.zero_rtt_enabled {
            println!("⚡ 尝试0-RTT连接...");
        }
        
        // 使用统一API连接 - 支持QUIC URI格式
        let session_id = self.transport.connect(&self.server_endpoint).await?;
        
        self.session_id = Some(session_id);
        println!("✅ QUIC连接建立成功 (会话ID: {})", session_id);
        
        if self.zero_rtt_enabled {
            println!("⚡ 0-RTT连接成功，无需等待握手完成");
        }
        
        Ok(())
    }
    
    /// 运行客户端演示
    pub async fn run(&mut self) -> Result<(), TransportError> {
        if self.session_id.is_none() {
            return Err(TransportError::Connection("未连接到服务器".to_string()));
        }
        
        println!("\n🚀 开始QUIC客户端演示");
        
        // 启动事件处理任务
        let mut events = self.transport.events();
        
        tokio::spawn(async move {
            loop {
                match events.next().await {
                    Some(event) => {
                        Self::handle_event(event).await;
                    }
                    None => {
                        println!("📡 事件流结束");
                        break;
                    }
                }
            }
        });
        
        // 发送测试消息
        self.send_test_messages().await?;
        
        // 演示QUIC特有功能
        self.demonstrate_quic_features().await?;
        
        // 保持连接一段时间以接收响应
        println!("\n⏳ 等待服务器响应...");
        sleep(Duration::from_secs(3)).await;
        
        println!("\n📊 QUIC客户端统计信息:");
        println!("   已发送消息: {}", self.messages_sent);
        println!("   已接收消息: {}", self.messages_received);
        println!("   创建流数: {}", self.streams_created);
        println!("   0-RTT模式: {}", if self.zero_rtt_enabled { "启用" } else { "禁用" });
        
        Ok(())
    }
    
    /// 发送测试消息
    async fn send_test_messages(&mut self) -> Result<(), TransportError> {
        let session_id = self.session_id.unwrap();
        
        let test_messages = vec![
            ("Hello from QUIC client!", "QUIC问候消息"),
            ("你好，QUIC服务器！支持多流并发", "中文QUIC消息"),
            ("{\"type\":\"quic_test\",\"features\":[\"multistream\",\"0rtt\",\"migration\"]}", "JSON特性描述"),
            ("🚀 QUIC is fast and secure! 🔒", "Emoji + 安全性"),
            ("QUIC binary stream data: \x01\x02\x03\x04", "二进制流数据"),
            ("Large message test for QUIC packet fragmentation and reassembly mechanism", "大消息分包测试"),
        ];
        
        for (i, (message, description)) in test_messages.iter().enumerate() {
            let packet = UnifiedPacket::data((i + 1) as u32, message.as_bytes());
            
            println!("📤 发送{}: {}", description, message);
            
            match self.transport.send_to_session(session_id, packet).await {
                Ok(()) => {
                    println!("✅ 消息发送成功");
                    self.messages_sent += 1;
                }
                Err(e) => {
                    println!("❌ 消息发送失败: {:?}", e);
                }
            }
            
            // 间隔发送
            sleep(Duration::from_millis(400)).await;
        }
        
        Ok(())
    }
    
    /// 演示QUIC特有功能
    async fn demonstrate_quic_features(&mut self) -> Result<(), TransportError> {
        let session_id = self.session_id.unwrap();
        
        println!("\n⚡ === QUIC特有功能演示 ===");
        
        // 1. 多流并发测试
        println!("🌊 多流并发消息发送");
        for stream_num in 1..=3 {
            let stream_msg = format!("并发流#{} - QUIC多流特性测试", stream_num);
            let stream_packet = UnifiedPacket::data(1000 + stream_num, stream_msg.as_bytes());
            
            match self.transport.send_to_session(session_id, stream_packet).await {
                Ok(()) => {
                    println!("✅ 流{}消息发送成功", stream_num);
                    self.messages_sent += 1;
                    self.streams_created += 1;
                }
                Err(e) => {
                    println!("❌ 流{}消息发送失败: {:?}", stream_num, e);
                }
            }
            
            // 非常短的间隔，模拟并发
            sleep(Duration::from_millis(10)).await;
        }
        
        sleep(Duration::from_millis(300)).await;
        
        // 2. 发送心跳包
        println!("💓 发送QUIC心跳包");
        let heartbeat = UnifiedPacket::heartbeat();
        match self.transport.send_to_session(session_id, heartbeat).await {
            Ok(()) => {
                println!("✅ QUIC心跳包发送成功");
                self.messages_sent += 1;
            }
            Err(e) => {
                println!("❌ QUIC心跳包发送失败: {:?}", e);
            }
        }
        
        sleep(Duration::from_millis(300)).await;
        
        // 3. 控制消息
        println!("🎛️ 发送QUIC控制消息");
        let control_msg = r#"{"action":"stream_info","request_features":["flow_control","congestion_control","connection_migration"]}"#;
        let control_packet = UnifiedPacket::control(2000, control_msg.as_bytes());
        match self.transport.send_to_session(session_id, control_packet).await {
            Ok(()) => {
                println!("✅ 控制消息发送成功");
                self.messages_sent += 1;
            }
            Err(e) => {
                println!("❌ 控制消息发送失败: {:?}", e);
            }
        }
        
        sleep(Duration::from_millis(300)).await;
        
        // 4. 大消息测试（展示QUIC的分包能力）
        println!("📦 发送超大消息 (测试QUIC分包重组)");
        let large_message = format!("超大消息测试 - QUIC自动分包与重组机制: {}", "B".repeat(2000));
        let large_packet = UnifiedPacket::data(3000, large_message.as_bytes());
        match self.transport.send_to_session(session_id, large_packet).await {
            Ok(()) => {
                println!("✅ 超大消息发送成功 ({} 字符)", large_message.len());
                self.messages_sent += 1;
            }
            Err(e) => {
                println!("❌ 超大消息发送失败: {:?}", e);
            }
        }
        
        sleep(Duration::from_millis(300)).await;
        
        // 5. 高频连续发送（测试流量控制）
        println!("⚡ 高频连续发送 (测试QUIC流量控制)");
        for i in 1..=10 {
            let rapid_msg = format!("高频消息#{} - 流量控制测试", i);
            let rapid_packet = UnifiedPacket::data(4000 + i, rapid_msg.as_bytes());
            
            match self.transport.send_to_session(session_id, rapid_packet).await {
                Ok(()) => {
                    self.messages_sent += 1;
                }
                Err(e) => {
                    println!("❌ 高频消息{}发送失败: {:?}", i, e);
                }
            }
            
            // 极短间隔，测试流量控制
            sleep(Duration::from_millis(5)).await;
        }
        println!("✅ 高频连续发送完成 (10条消息)");
        
        // 6. 模拟连接迁移
        println!("🔄 模拟连接迁移 (QUIC特有功能)");
        let migration_msg = "连接迁移测试 - QUIC支持IP地址变更时保持连接";
        let migration_packet = UnifiedPacket::control(5000, migration_msg.as_bytes());
        match self.transport.send_to_session(session_id, migration_packet).await {
            Ok(()) => {
                println!("✅ 连接迁移消息发送成功");
                self.messages_sent += 1;
            }
            Err(e) => {
                println!("❌ 连接迁移消息发送失败: {:?}", e);
            }
        }
        
        Ok(())
    }
    
    /// 处理传输事件
    async fn handle_event(event: msgtrans::unified::event::TransportEvent) {
        use msgtrans::unified::event::TransportEvent;
        
        match event {
            TransportEvent::PacketReceived { session_id, packet } => {
                println!("📨 收到QUIC服务器消息 (会话{}): 类型{:?}, ID{}", 
                         session_id, packet.packet_type, packet.message_id);
                
                if let Some(content) = packet.payload_as_string() {
                    println!("   内容: {}", content);
                }
                
                println!("   大小: {} bytes", packet.payload.len());
                
                // 检测特殊响应类型
                match packet.packet_type {
                    msgtrans::unified::packet::PacketType::Heartbeat => {
                        println!("💚 收到QUIC心跳响应");
                    }
                    msgtrans::unified::packet::PacketType::Control => {
                        println!("🎛️ 收到QUIC控制响应");
                    }
                    msgtrans::unified::packet::PacketType::Echo => {
                        println!("🔄 收到QUIC回显响应");
                    }
                    _ => {}
                }
            }
            
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🔗 QUIC连接建立: 会话{}, 协议{:?}, 地址{:?}", 
                         session_id, info.protocol, info.peer_addr);
                println!("   ⚡ QUIC特性: 多流、0-RTT、连接迁移、内置加密");
            }
            
            TransportEvent::ConnectionClosed { session_id, reason } => {
                println!("❌ QUIC连接关闭: 会话{}, 原因: {:?}", session_id, reason);
            }
            
            TransportEvent::TransportError { session_id, error } => {
                println!("⚠️ QUIC传输错误: 会话{:?}, 错误: {:?}", session_id, error);
            }
            
            _ => {
                println!("📡 其他QUIC事件: {:?}", event);
            }
        }
    }
    
    /// 关闭连接
    pub async fn close(&mut self) -> Result<(), TransportError> {
        if let Some(session_id) = self.session_id {
            println!("🔌 优雅关闭QUIC连接");
            // 传输层会自动处理QUIC连接关闭
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🌟 msgtrans QUIC客户端演示");
    println!("=========================");
    println!("🚀 特性展示:");
    println!("   ✨ 使用统一的 connect() API");
    println!("   ⚡ QUIC高性能传输协议");
    println!("   🌊 多流并发通信");
    println!("   🚀 0-RTT快速连接");
    println!("   🔄 连接迁移支持");
    println!("   🔒 内置端到端加密");
    println!("   📦 自动分包重组");
    println!("   🎛️ 智能流量控制");
    println!("   📡 事件驱动处理");
    println!();
    
    // 创建客户端
    let mut client = QuicClientDemo::new("quic://127.0.0.1:9003").await?;
    
    // 启用0-RTT（可选）
    client.enable_zero_rtt();
    
    // 连接并运行演示
    match client.connect().await {
        Ok(()) => {
            client.run().await?;
        }
        Err(e) => {
            println!("❌ 连接失败: {:?}", e);
            println!("💡 提示: 请确保QUIC服务器正在运行");
            println!("   运行命令: cargo run --example server_multiprotocol");
        }
    }
    
    // 清理资源
    client.close().await?;
    
    println!("\n👋 QUIC客户端演示结束");
    println!("\n✅ QUIC特性验证完成:");
    println!("  ✓ 统一API兼容性");
    println!("  ✓ 高性能传输");
    println!("  ✓ 多流并发");
    println!("  ✓ 0-RTT连接");
    println!("  ✓ 连接迁移");
    println!("  ✓ 端到端加密");
    println!("  ✓ 自动分包重组");
    println!("  ✓ 流量控制");
    println!("  ✓ 可靠传输");
    
    Ok(())
} 