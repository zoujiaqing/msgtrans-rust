/// TCP客户端演示 - 使用msgtrans统一架构
/// 
/// 展示如何使用msgtrans统一架构创建TCP客户端
/// 演示统一API的使用方法和事件驱动处理

use std::time::Duration;
use std::sync::Arc;
use tokio::time::sleep;
use tokio::sync::Mutex;
use futures::StreamExt;

use msgtrans::unified::{
    Transport, TransportBuilder,
    packet::UnifiedPacket,
    error::TransportError,
    config::TransportConfig,
};

/// TCP客户端演示
pub struct TcpClientDemo {
    transport: Transport,
    session_id: Option<u64>,
    server_addr: String,
    messages_sent: u64,
    messages_received: Arc<Mutex<u64>>,
}

impl TcpClientDemo {
    /// 创建新的TCP客户端演示
    pub async fn new(server_addr: &str) -> Result<Self, TransportError> {
        println!("🌟 TCP客户端演示 - 使用msgtrans统一架构");
        println!("=======================================");
        
        // 使用统一架构创建传输层
        let config = TransportConfig::default();
        let transport = TransportBuilder::new()
            .config(config)
            .build()
            .await?;
        
        Ok(Self {
            transport,
            session_id: None,
            server_addr: server_addr.to_string(),
            messages_sent: 0,
            messages_received: Arc::new(Mutex::new(0)),
        })
    }
    
    /// 连接到服务器
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        println!("🔌 连接到TCP服务器: {}", self.server_addr);
        
        // 使用统一API连接 - 支持URI格式
        let uri = format!("tcp://{}", self.server_addr);
        let session_id = self.transport.connect(&uri).await?;
        
        self.session_id = Some(session_id);
        println!("✅ TCP连接建立成功 (会话ID: {})", session_id);
        
        Ok(())
    }
    
    /// 运行客户端演示
    pub async fn run(&mut self) -> Result<(), TransportError> {
        if self.session_id.is_none() {
            return Err(TransportError::Connection("未连接到服务器".to_string()));
        }
        
        println!("\n🚀 开始TCP客户端演示");
        
        // 启动事件处理任务
        let mut events = self.transport.events();
        let messages_received = self.messages_received.clone();
        
        tokio::spawn(async move {
            loop {
                match events.next().await {
                    Some(event) => {
                        Self::handle_event(event, &messages_received).await;
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
        
        // 保持连接一段时间以接收响应
        println!("\n⏳ 等待服务器响应...");
        sleep(Duration::from_secs(5)).await; // 增加等待时间
        
        let received_count = *self.messages_received.lock().await;
        println!("\n📊 客户端统计信息:");
        println!("   已发送消息: {}", self.messages_sent);
        println!("   已接收消息: {}", received_count);
        
        Ok(())
    }
    
    /// 发送测试消息
    async fn send_test_messages(&mut self) -> Result<(), TransportError> {
        let session_id = self.session_id.unwrap();
        
        let test_messages = vec![
            ("Hello from TCP client!", "问候消息"),
            ("你好，服务器！这是中文测试", "中文消息"),
            ("{\"type\":\"ping\",\"timestamp\":\"2024-01-01T00:00:00Z\"}", "JSON消息"),
            ("Binary data: \x01\x02\x03\x04", "二进制数据"),
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
            sleep(Duration::from_millis(500)).await;
        }
        
        // 发送心跳包
        println!("💓 发送心跳包");
        let heartbeat = UnifiedPacket::heartbeat();
        match self.transport.send_to_session(session_id, heartbeat).await {
            Ok(()) => {
                println!("✅ 心跳包发送成功");
                self.messages_sent += 1;
            }
            Err(e) => {
                println!("❌ 心跳包发送失败: {:?}", e);
            }
        }
        
        Ok(())
    }
    
    /// 处理传输事件
    async fn handle_event(
        event: msgtrans::unified::event::TransportEvent,
        messages_received: &Arc<Mutex<u64>>
    ) {
        use msgtrans::unified::event::TransportEvent;
        
        match event {
            TransportEvent::PacketReceived { session_id, packet } => {
                println!("📨 收到服务器消息 (会话{}): 类型{:?}, ID{}", 
                         session_id, packet.packet_type, packet.message_id);
                
                if let Some(content) = packet.payload_as_string() {
                    println!("   内容: {}", content);
                }
                
                println!("   大小: {} bytes", packet.payload.len());
                
                // 更新接收计数器
                {
                    let mut count = messages_received.lock().await;
                    *count += 1;
                    println!("✅ 已接收消息总数: {}", *count);
                }
            }
            
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🔗 连接建立: 会话{}, 协议{:?}, 地址{:?}", 
                         session_id, info.protocol, info.peer_addr);
            }
            
            TransportEvent::ConnectionClosed { session_id, reason } => {
                println!("❌ 连接关闭: 会话{}, 原因: {:?}", session_id, reason);
            }
            
            TransportEvent::TransportError { session_id, error } => {
                println!("⚠️ 传输错误: 会话{:?}, 错误: {:?}", session_id, error);
            }
            
            _ => {
                println!("📡 其他事件: {:?}", event);
            }
        }
    }
    
    /// 关闭连接
    pub async fn close(&mut self) -> Result<(), TransportError> {
        if let Some(_session_id) = self.session_id {
            println!("🔌 关闭TCP连接");
            // 传输层会自动处理连接清理
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🌟 msgtrans TCP客户端演示");
    println!("=======================");
    println!("🚀 特性展示:");
    println!("   ✨ 使用统一的 connect() API");
    println!("   🔧 自动协议检测和处理");
    println!("   📡 事件驱动的消息处理");
    println!("   💓 心跳和错误处理");
    println!("   🌐 支持多种数据格式");
    println!();
    
    // 创建客户端
    let mut client = TcpClientDemo::new("127.0.0.1:9001").await?;
    
    // 连接并运行演示
    match client.connect().await {
        Ok(()) => {
            client.run().await?;
        }
        Err(e) => {
            println!("❌ 连接失败: {:?}", e);
            println!("💡 提示: 请确保服务器正在运行 (cargo run --example server_multiprotocol)");
        }
    }
    
    // 清理资源
    client.close().await?;
    
    println!("\n👋 客户端演示结束");
    
    Ok(())
} 