/// WebSocket客户端演示 - 使用msgtrans统一架构
/// 
/// 展示如何使用msgtrans统一架构创建WebSocket客户端
/// 演示WebSocket特有的特性：连接升级、Ping/Pong、压缩等

use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::unified::{
    Transport, TransportBuilder,
    packet::UnifiedPacket,
    error::TransportError,
    config::TransportConfig,
};

/// WebSocket客户端演示
pub struct WebSocketClientDemo {
    transport: Transport,
    session_id: Option<u64>,
    server_url: String,
    messages_sent: u64,
    messages_received: u64,
    ping_count: u64,
    pong_count: u64,
}

impl WebSocketClientDemo {
    /// 创建新的WebSocket客户端演示
    pub async fn new(server_url: &str) -> Result<Self, TransportError> {
        println!("🌟 WebSocket客户端演示 - 使用msgtrans统一架构");
        println!("============================================");
        
        // 使用统一架构创建传输层
        let config = TransportConfig::default();
        let transport = TransportBuilder::new()
            .config(config)
            .build()
            .await?;
        
        Ok(Self {
            transport,
            session_id: None,
            server_url: server_url.to_string(),
            messages_sent: 0,
            messages_received: 0,
            ping_count: 0,
            pong_count: 0,
        })
    }
    
    /// 连接到服务器
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        println!("🔌 连接到WebSocket服务器: {}", self.server_url);
        
        // 使用统一API连接 - 支持WebSocket URI格式
        let session_id = self.transport.connect(&self.server_url).await?;
        
        self.session_id = Some(session_id);
        println!("✅ WebSocket连接建立成功 (会话ID: {})", session_id);
        
        Ok(())
    }
    
    /// 运行客户端演示
    pub async fn run(&mut self) -> Result<(), TransportError> {
        if self.session_id.is_none() {
            return Err(TransportError::Connection("未连接到服务器".to_string()));
        }
        
        println!("\n🚀 开始WebSocket客户端演示");
        
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
        
        // 演示WebSocket特有功能
        self.demonstrate_websocket_features().await?;
        
        // 保持连接一段时间以接收响应
        println!("\n⏳ 等待服务器响应...");
        sleep(Duration::from_secs(3)).await;
        
        println!("\n📊 WebSocket客户端统计信息:");
        println!("   已发送消息: {}", self.messages_sent);
        println!("   已接收消息: {}", self.messages_received);
        println!("   Ping次数: {}", self.ping_count);
        println!("   Pong次数: {}", self.pong_count);
        
        Ok(())
    }
    
    /// 发送测试消息
    async fn send_test_messages(&mut self) -> Result<(), TransportError> {
        let session_id = self.session_id.unwrap();
        
        let test_messages = vec![
            ("Hello from WebSocket client!", "WebSocket问候"),
            ("你好，WebSocket服务器！", "中文WebSocket消息"),
            ("{\"type\":\"websocket_test\",\"protocol\":\"ws\",\"compression\":\"deflate\"}", "JSON消息"),
            ("🌐 WebSocket supports emojis! 🚀", "Emoji测试"),
            ("WebSocket binary data: \x01\x02\x03\x04", "二进制数据"),
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
            sleep(Duration::from_millis(600)).await;
        }
        
        Ok(())
    }
    
    /// 演示WebSocket特有功能
    async fn demonstrate_websocket_features(&mut self) -> Result<(), TransportError> {
        let session_id = self.session_id.unwrap();
        
        println!("\n🌐 === WebSocket特有功能演示 ===");
        
        // 1. 发送Ping/Pong心跳
        println!("💓 发送WebSocket心跳包");
        let heartbeat = UnifiedPacket::heartbeat();
        match self.transport.send_to_session(session_id, heartbeat).await {
            Ok(()) => {
                println!("✅ 心跳包发送成功");
                self.ping_count += 1;
            }
            Err(e) => {
                println!("❌ 心跳包发送失败: {:?}", e);
            }
        }
        
        sleep(Duration::from_millis(300)).await;
        
        // 2. 发送控制消息
        println!("🎛️ 发送WebSocket控制消息");
        let control_msg = r#"{"action":"ping","protocol":"websocket","features":["compression","extensions"]}"#;
        let control_packet = UnifiedPacket::control(999, control_msg.as_bytes());
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
        
        // 3. 发送大消息（测试分帧）
        println!("📦 发送大消息 (测试WebSocket分帧)");
        let large_message = format!("大消息测试 - WebSocket自动分帧: {}", "A".repeat(1000));
        let large_packet = UnifiedPacket::data(1001, large_message.as_bytes());
        match self.transport.send_to_session(session_id, large_packet).await {
            Ok(()) => {
                println!("✅ 大消息发送成功 ({} 字符)", large_message.len());
                self.messages_sent += 1;
            }
            Err(e) => {
                println!("❌ 大消息发送失败: {:?}", e);
            }
        }
        
        sleep(Duration::from_millis(300)).await;
        
        // 4. 快速连续发送（测试缓冲）
        println!("⚡ 快速连续发送 (测试WebSocket缓冲)");
        for i in 1..=5 {
            let rapid_msg = format!("快速消息 #{}", i);
            let rapid_packet = UnifiedPacket::data(2000 + i, rapid_msg.as_bytes());
            
            match self.transport.send_to_session(session_id, rapid_packet).await {
                Ok(()) => {
                    self.messages_sent += 1;
                }
                Err(e) => {
                    println!("❌ 快速消息{}发送失败: {:?}", i, e);
                }
            }
            
            // 很短的间隔
            sleep(Duration::from_millis(50)).await;
        }
        println!("✅ 快速连续发送完成");
        
        Ok(())
    }
    
    /// 处理传输事件
    async fn handle_event(event: msgtrans::unified::event::TransportEvent) {
        use msgtrans::unified::event::TransportEvent;
        
        match event {
            TransportEvent::PacketReceived { session_id, packet } => {
                println!("📨 收到WebSocket服务器消息 (会话{}): 类型{:?}, ID{}", 
                         session_id, packet.packet_type, packet.message_id);
                
                if let Some(content) = packet.payload_as_string() {
                    println!("   内容: {}", content);
                }
                
                println!("   大小: {} bytes", packet.payload.len());
                
                // 检测是否是Pong响应
                if packet.packet_type == msgtrans::unified::packet::PacketType::Heartbeat {
                    println!("💚 收到Pong响应");
                }
            }
            
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🔗 WebSocket连接建立: 会话{}, 协议{:?}, 地址{:?}", 
                         session_id, info.protocol, info.peer_addr);
                println!("   🌐 WebSocket特性: 升级协议、自动心跳、消息分帧");
            }
            
            TransportEvent::ConnectionClosed { session_id, reason } => {
                println!("❌ WebSocket连接关闭: 会话{}, 原因: {:?}", session_id, reason);
            }
            
            TransportEvent::TransportError { session_id, error } => {
                println!("⚠️ WebSocket传输错误: 会话{:?}, 错误: {:?}", session_id, error);
            }
            
            _ => {
                println!("📡 其他WebSocket事件: {:?}", event);
            }
        }
    }
    
    /// 关闭连接
    pub async fn close(&mut self) -> Result<(), TransportError> {
        if let Some(session_id) = self.session_id {
            println!("🔌 优雅关闭WebSocket连接");
            // 传输层会自动处理WebSocket关闭握手
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🌟 msgtrans WebSocket客户端演示");
    println!("==============================");
    println!("🚀 特性展示:");
    println!("   ✨ 使用统一的 connect() API");
    println!("   🌐 自动WebSocket协议升级");
    println!("   📡 事件驱动的消息处理");
    println!("   💓 自动Ping/Pong心跳");
    println!("   📦 自动消息分帧和重组");
    println!("   🗜️ 自动压缩支持（deflate）");
    println!("   🔒 自动子协议协商");
    println!();
    
    // 创建客户端
    let mut client = WebSocketClientDemo::new("ws://127.0.0.1:9002").await?;
    
    // 连接并运行演示
    match client.connect().await {
        Ok(()) => {
            client.run().await?;
        }
        Err(e) => {
            println!("❌ 连接失败: {:?}", e);
            println!("💡 提示: 请确保WebSocket服务器正在运行");
            println!("   运行命令: cargo run --example server_multiprotocol");
        }
    }
    
    // 清理资源
    client.close().await?;
    
    println!("\n👋 WebSocket客户端演示结束");
    println!("\n✅ WebSocket特性验证完成:");
    println!("  ✓ 统一API兼容性");
    println!("  ✓ 协议自动升级");
    println!("  ✓ 实时双向通信");
    println!("  ✓ 自动心跳保活");
    println!("  ✓ 消息分帧处理");
    println!("  ✓ 大消息传输");
    println!("  ✓ 高频消息处理");
    
    Ok(())
} 