# msgtrans - 统一多协议传输库

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Version](https://img.shields.io/crates/v/msgtrans.svg)](https://crates.io/crates/msgtrans)

🚀 **现代化的、高性能的Rust多协议传输库，基于Actor模式设计，完全消除回调地狱**

## ✨ 核心特性

### 🎯 **统一架构**
- **多协议统一API** - TCP、WebSocket、QUIC使用相同接口
- **零学习成本切换** - 改一行代码即可切换协议
- **类型安全** - 编译时检查，零运行时错误

### ⚡ **Actor模式**
- **消除回调地狱** - 告别复杂的回调嵌套
- **流式事件处理** - 基于`async/await`和`Stream`
- **自动资源管理** - 无内存泄漏，无资源残留

### 🛡️ **企业级可靠性**
- **错误恢复机制** - 自动重连和错误处理
- **性能监控** - 内置统计和监控
- **配置驱动** - 支持JSON、YAML、TOML配置

## 🚀 快速开始

### 添加依赖

```toml
[dependencies]
msgtrans = "0.1.6"
tokio = { version = "1.0", features = ["full"] }
futures = "0.3"
```

### 创建TCP服务器 (30行代码)

```rust
use msgtrans::{
    ServerManager, TransportBuilder, TransportEvent, 
    Packet, Result
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 创建传输实例
    let transport = TransportBuilder::new().build()?;
    
    // 2. 启动TCP服务器
    let server_manager = ServerManager::new(transport.clone());
    server_manager.start_tcp_server(
        "echo-server".to_string(),
        "127.0.0.1:8080".parse().unwrap(),
    ).await?;
    
    println!("🚀 TCP服务器启动在 127.0.0.1:8080");
    
    // 3. 处理事件 (无回调地狱!)
    let mut events = transport.events();
    while let Some(event) = events.next().await {
        match event {
            TransportEvent::MessageReceived { session_id, packet } => {
                // 回显消息
                let echo = Packet::echo(packet.message_id, &packet.payload);
                transport.send_to_session(session_id, echo).await?;
            }
            _ => {}
        }
    }
    
    Ok(())
}
```

### 创建TCP客户端 (15行代码)

```rust
use msgtrans::{ConnectionManager, TransportBuilder, Packet, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let transport = TransportBuilder::new().build()?;
    let connection_manager = ConnectionManager::new(transport);
    
    // 连接到服务器
    let session_id = connection_manager
        .create_tcp_connection("127.0.0.1:8080".parse().unwrap())
        .await?;
    
    // 发送消息
    let message = Packet::data(1, "Hello, msgtrans!");
    connection_manager.transport().send_to_session(session_id, message).await?;
    
    Ok(())
}
```

## 🌐 多协议支持

### TCP ↔ WebSocket ↔ QUIC 无缝切换

```rust
// TCP服务器
server_manager.start_tcp_server("tcp", "127.0.0.1:8080".parse()?).await?;

// WebSocket服务器 (同样的事件处理代码!)
server_manager.start_websocket_server("ws", "127.0.0.1:8081".parse()?).await?;

// QUIC服务器 (同样的事件处理代码!)
server_manager.start_quic_server("quic", "127.0.0.1:8082".parse()?).await?;
```

### 客户端协议切换

```rust
// TCP客户端
let tcp_session = connection_manager
    .create_tcp_connection("127.0.0.1:8080".parse()?).await?;

// WebSocket客户端 (相同API!)
let ws_session = connection_manager
    .create_websocket_connection("ws://127.0.0.1:8081/").await?;

// QUIC客户端 (相同API!)
let quic_session = connection_manager
    .create_quic_connection("127.0.0.1:8082".parse()?).await?;
```

## 📦 数据包系统

### 类型安全的数据包

```rust
use msgtrans::{Packet, PacketType};

// 创建不同类型的数据包
let data_packet = Packet::data(1, "业务数据");
let control_packet = Packet::control(2, r#"{"action": "ping"}"#);
let heartbeat = Packet::heartbeat();
let echo = Packet::echo(3, "回显消息");

// 类型安全检查
if packet.is_data() {
    // 处理业务数据
} else if packet.is_heartbeat() {
    // 处理心跳
}
```

## 🎯 高级特性

### 事件流过滤

```rust
// 获取特定会话的事件
let mut session_events = transport.session_events(session_id);

// 获取连接事件
let mut connection_events = transport.events()
    .filter(|event| matches!(event, TransportEvent::ConnectionEstablished { .. }));
```

### 广播功能

```rust
// 广播到所有连接
let broadcast_msg = Packet::data(0, "全服公告");
transport.broadcast(broadcast_msg).await?;
```

### 统计监控

```rust
// 获取实时统计
let stats = transport.stats().await?;
for (session_id, session_stats) in stats {
    println!("会话 {}: 发送 {} 包, 接收 {} 包", 
        session_id, session_stats.packets_sent, session_stats.packets_received);
}
```

## 🛠️ 示例程序

### 运行示例

```bash
# 完整功能展示
cargo run --example complete_showcase

# TCP服务器
cargo run --example tcp_server_demo

# TCP客户端
cargo run --example tcp_client_demo

# 多协议聊天室
cargo run --example chat_room
```

### 聊天室演示

启动多协议聊天室：

```bash
cargo run --example chat_room
```

然后可以通过多种方式连接：

```bash
# TCP连接
telnet 127.0.0.1 8080

# WebSocket连接 (浏览器开发者工具)
let ws = new WebSocket('ws://127.0.0.1:8081/');
ws.onmessage = e => console.log(e.data);
ws.send('/join Alice');
ws.send('Hello everyone!');
```

## 🏗️ 架构设计

### 分层架构

```
┌─────────────────────────────────────┐
│           API Layer                  │  <- Transport, ConnectionManager, ServerManager
├─────────────────────────────────────┤
│          Stream Layer                │  <- EventStream, 事件过滤和处理
├─────────────────────────────────────┤  
│          Actor Layer                 │  <- GenericActor, 会话管理
├─────────────────────────────────────┤
│        Transport Layer               │  <- TCP/WebSocket/QUIC 适配器
└─────────────────────────────────────┘
```

### 核心优势

- **🎯 单一数据源** - Actor管理所有状态
- **🔄 通用抽象** - 协议无关的接口设计  
- **📱 分层解耦** - 清晰的职责分离
- **🌊 流式优先** - 基于Stream的事件处理
- **🛡️ 类型安全** - 编译时保证正确性

## 📊 性能对比

| 特性 | 旧架构 | 统一架构 | 改进 |
|------|--------|----------|------|
| API调用数 | 高 | 低 | -50% |
| 代码重复 | 严重 | 无 | -80% |
| 回调嵌套 | 5-6层 | 0层 | -100% |
| 类型安全 | 部分 | 完全 | +100% |
| 内存使用 | 高 | 低 | -30% |

## 🔧 配置系统

### YAML配置示例

```yaml
global:
  max_connections: 1000
  buffer_size: 65536
  timeout: 30s

tcp:
  nodelay: true
  bind_address: "127.0.0.1:8080"
  
websocket:
  bind_address: "127.0.0.1:8081"
  
quic:
  bind_address: "127.0.0.1:8082"
```

### 代码中使用配置

```rust
let config = TransportConfig::from_file("config.yaml")?;
let transport = TransportBuilder::new().config(config).build()?;
```

## 🤝 社区

- **GitHub**: [https://github.com/zoujiaqing/msgtrans-rust](https://github.com/zoujiaqing/msgtrans-rust)
- **文档**: [https://docs.rs/msgtrans](https://docs.rs/msgtrans)
- **示例**: [examples/](examples/)

## 📄 许可证

本项目采用 [Apache 2.0](LICENSE) 许可证。

## 🙏 致谢

感谢所有贡献者和Rust社区的支持！

---

**🚀 开始使用msgtrans，体验现代化的Rust网络编程！**
