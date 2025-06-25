 # MsgTrans 架构设计 v2.0

## 🎯 设计理念

**MsgTrans 是一个统一的多协议传输框架，核心理念是：**
> "一套业务代码 + 不同配置 = 支持任意协议"

用户通过配置驱动的方式获得协议支持，框架隐藏所有协议实现细节。

## 🏗️ 架构分层

```
┌─────────────────────────────────────┐
│        User Application             │  用户业务代码
├─────────────────────────────────────┤
│ TransportClient   TransportServer   │  用户直接操作层
│  - send()          - send_to()      │  
│  - connect()       - broadcast()    │  
│  - disconnect()    - sessions[]     │  ← 连接管理在 TransportServer
├─────────────────────────────────────┤
│           Transport                 │  协议抽象层 (用户不可见)
│      单一连接的传输抽象               │  
│  - send_message()  - recv_message() │
├─────────────────────────────────────┤
│       Protocol Adapters            │  协议适配器 (用户不可见)
│   TcpAdapter/WsAdapter/QuicAdapter  │
├─────────────────────────────────────┤
│         Raw Connections             │  底层连接 (用户不可见)
│    TcpStream/WebSocket/QuicConn     │
└─────────────────────────────────────┘
```

## 📡 核心组件设计

### 1. TransportClient (客户端传输层)

**职责**: 单一连接管理 + 客户端特性

```rust
pub struct TransportClient {
    transport: Transport,                    // 内部管理单一Transport
    pool_config: ConnectionPoolConfig,       // 连接池配置
    retry_config: RetryConfig,              // 重试策略
    current_session_id: Option<SessionId>,  // 当前会话ID (内部使用)
}

impl TransportClient {
    // 🎯 简化API - 用户无需关心session_id
    pub async fn connect(&mut self) -> Result<()>;
    pub async fn disconnect(&mut self) -> Result<()>;
    pub async fn send(&self, packet: Packet) -> Result<()>;
    pub fn is_connected(&self) -> bool;
    
    // 🔧 高级API (可选)
    pub async fn send(&self, session_id: SessionId, packet: Packet) -> Result<()>;
    pub async fn close_session(&self, session_id: SessionId) -> Result<()>;
}
```

### 2. TransportServer (服务端传输层)

**职责**: 多连接管理 + 服务端特性

```rust
pub struct TransportServer {
    sessions: HashMap<SessionId, Transport>, // ✅ 连接数组在这里管理
    acceptor_config: AcceptorConfig,         // 接受器配置
    middleware_stack: Vec<Box<dyn ServerMiddleware>>, // 中间件
    rate_limiter: Option<RateLimiterConfig>, // 限流器
    graceful_shutdown: Option<Duration>,     // 优雅关闭
}

impl TransportServer {
    // 🎯 多会话API - 用户需要指定session_id
    pub async fn send_to(&self, session_id: SessionId, packet: Packet) -> Result<()>;
    pub async fn broadcast(&self, packet: Packet) -> Result<()>;
    pub async fn active_sessions(&self) -> Vec<SessionId>;
    pub async fn close_session(&self, session_id: SessionId) -> Result<()>;
    
    // 🚀 服务启动
    pub async fn serve(self) -> Result<()>;
}
```

### 3. Transport (协议抽象层)

**职责**: 单一连接的协议无关传输 + 会话抽象

```rust
pub struct Transport {
    session_id: SessionId,                          // 会话标识
    protocol_adapter: Box<dyn ProtocolAdapter>,    // 协议适配器
    connection_info: ConnectionInfo,                // 连接信息
    event_stream: EventStream,                      // 事件流
}

impl Transport {
    // 🔧 协议无关的传输能力 (框架内部使用)
    pub async fn send_message(&mut self, data: Bytes) -> Result<()>;
    pub async fn recv_message(&mut self) -> Result<Option<Bytes>>;
    pub fn session_id(&self) -> SessionId;
    pub fn connection_info(&self) -> &ConnectionInfo;
    pub fn events(&self) -> EventStream;
}
```

## 🔧 Builder 模式设计

### TransportClientBuilder

```rust
let client = TransportClientBuilder::new()
    .with_protocol(TcpClientConfig::new("server:8080"))
    .connect_timeout(Duration::from_secs(30))
    .retry_strategy(RetryConfig::exponential_backoff(3, Duration::from_millis(100)))
    .build().await?;
```

### TransportServerBuilder

```rust
let server = TransportServerBuilder::new()
    .with_protocol(TcpServerConfig::new("0.0.0.0:8080"))
    .with_protocol(WebSocketServerConfig::new("0.0.0.0:8081"))
    .max_connections(10000)
    .with_middleware(LoggingMiddleware::new())
    .with_middleware(AuthMiddleware::new())
    .rate_limiter(RateLimiterConfig::new(1000, 100))
    .build().await?;
```

## 🎯 协议配置驱动

### 配置类型

```rust
// TCP协议配置
TcpClientConfig::new("server:8080")
TcpServerConfig::new("0.0.0.0:8080")

// WebSocket协议配置  
WebSocketClientConfig::new("ws://server:8080/ws")
WebSocketServerConfig::new("0.0.0.0:8080")

// QUIC协议配置
QuicClientConfig::new("server:4433")
QuicServerConfig::new("0.0.0.0:4433")
```

### 多协议支持

```rust
// 服务端可以同时监听多个协议
let server = TransportServerBuilder::new()
    .with_protocol(TcpServerConfig::new("0.0.0.0:8080"))     // TCP在8080
    .with_protocol(WebSocketServerConfig::new("0.0.0.0:8081")) // WS在8081
    .with_protocol(QuicServerConfig::new("0.0.0.0:8082"))    // QUIC在8082
    .build().await?;

// 同样的业务逻辑处理所有协议的连接
server.serve().await?;
```

## 📨 使用示例

### 客户端使用

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // 创建客户端 (TCP协议)
    let mut client = TransportClientBuilder::new()
        .with_protocol(TcpClientConfig::new("127.0.0.1:8080"))
        .build().await?;
    
    // 连接服务器
    client.connect().await?;
    
    // 发送消息 (无需session_id)
    let packet = Packet::text("Hello, Server!");
    client.send(packet).await?;
    
    // 接收消息
    let mut events = client.events();
    while let Some(event) = events.next().await {
        match event {
            TransportEvent::MessageReceived { packet, .. } => {
                println!("收到回复: {}", packet.payload_as_string().unwrap());
                break;
            }
            _ => {}
        }
    }
    
    client.disconnect().await?;
    Ok(())
}
```

### 服务端使用

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // 创建多协议服务器
    let server = TransportServerBuilder::new()
        .with_protocol(TcpServerConfig::new("0.0.0.0:8080"))
        .with_protocol(WebSocketServerConfig::new("0.0.0.0:8081"))
        .build().await?;
    
    // 启动事件处理
    let mut events = server.events();
    let event_server = server.clone();
    
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                TransportEvent::ConnectionEstablished { session_id, .. } => {
                    println!("新连接: {}", session_id);
                }
                TransportEvent::MessageReceived { session_id, packet } => {
                    println!("收到消息: {}", packet.payload_as_string().unwrap());
                    
                    // Echo回复 (需要指定session_id)
                    let echo = Packet::echo(packet.message_id, packet.payload);
                    event_server.send_to(session_id, echo).await.unwrap();
                }
                TransportEvent::ConnectionClosed { session_id, .. } => {
                    println!("连接关闭: {}", session_id);
                }
                _ => {}
            }
        }
    });
    
    // 启动服务器
    server.serve().await?;
    Ok(())
}
```

## 🔌 协议扩展机制

### 新增协议支持

1. **实现协议配置**:
```rust
#[derive(Debug, Clone)]
pub struct CustomProtocolConfig {
    pub endpoint: String,
    pub custom_options: HashMap<String, String>,
}

impl ProtocolConfig for CustomProtocolConfig { /* ... */ }
impl DynProtocolConfig for CustomProtocolConfig { /* ... */ }
```

2. **实现协议适配器**:
```rust
pub struct CustomProtocolAdapter {
    connection: CustomConnection,
    config: CustomProtocolConfig,
}

impl ProtocolAdapter for CustomProtocolAdapter { /* ... */ }
```

3. **用户使用**:
```rust
let client = TransportClientBuilder::new()
    .with_protocol(CustomProtocolConfig::new("custom://server:9999"))
    .build().await?;
    
// 使用方式完全相同
client.send(packet).await?;
```

## 🎯 设计优势

1. **协议透明**: 用户代码与协议无关，通过配置切换协议
2. **统一接口**: 客户端/服务端提供直观的API，隐藏复杂性
3. **职责清晰**: 每层专注自己的职责，易于维护和扩展
4. **配置驱动**: 新增协议支持不影响用户代码
5. **类型安全**: 编译时检查，运行时高性能

## 🚀 实现要点

1. **Transport 是内部抽象**: 用户永远不直接操作 Transport 对象
2. **连接管理分离**: TransportClient管理单连接，TransportServer管理多连接
3. **事件驱动**: 统一的事件流模型，支持异步消息处理
4. **中间件支持**: 服务端支持可插拔的中间件栈
5. **协议插件化**: 通过 trait 和配置系统支持协议扩展

## 🚀 性能优化路径

### ⚠️ 当前架构性能问题

**主要问题**：
1. **锁竞争瓶颈**: `Arc<RwLock<HashMap<SessionId, Transport>>>` 在高并发下成为性能瓶颈
2. **内存使用过量**: 每个连接创建完整 Transport 实例，内存开销大（每连接1-2KB开销）
3. **广播性能灾难**: 广播操作需要遍历所有连接，O(n) 复杂度不可接受
4. **单机扩展极限**: 无法水平扩展，受限于单机资源

### 🎯 三阶段优化计划

#### 第一阶段：紧急修复 (当前优先级)
**目标**: 修复基础架构问题，确保功能正确性

1. **简化方法签名**
   - 移除 `start_tcp_server` 等方法中不必要的参数传递
   - 直接使用 `&self` 访问 sessions 和 session_id_generator

2. **实现连接处理逻辑**
   - 在协议服务器中实现真正的连接接受循环
   - 添加 `add_session()` 方法统一处理新会话创建
   - 实现从原始连接创建 Transport 实例的机制

3. **会话生命周期管理**
   - 自动生成 SessionId 并注册到 sessions 映射
   - 处理连接断开时的会话清理

#### 第二阶段：性能优化 (中期目标)
**目标**: 解决性能瓶颈，支持中等规模并发

1. **分片架构**
   ```rust
   struct ShardedSessions {
       shards: Vec<Arc<RwLock<HashMap<SessionId, Transport>>>>,
       shard_count: usize,
   }
   ```

2. **高效广播机制**
   ```rust
   struct BroadcastManager {
       broadcast_tx: broadcast::Sender<Packet>,
       connections: DashMap<SessionId, broadcast::Receiver<Packet>>,
   }
   ```

3. **连接池复用**
   ```rust
   struct ConnectionPool {
       available: mpsc::UnboundedReceiver<ReusableConnection>,
       in_use: DashMap<SessionId, ReusableConnection>,
   }
   ```

#### 第三阶段：架构重构 (长期目标)  
**目标**: 完全无锁设计，支持大规模高并发

1. **Actor 化连接管理**
   ```rust
   struct ConnectionActor {
       id: SessionId,
       command_rx: mpsc::Receiver<Command>,
       transport: LightweightTransport,
   }
   ```

2. **无锁服务器设计**
   ```rust
   struct ServerTransport {
       connection_senders: DashMap<SessionId, mpsc::Sender<Command>>,
       broadcast_manager: BroadcastManager,
   }
   ```

3. **水平扩展架构**
   - 支持多机分布式部署
   - 连接状态分片存储
   - 跨节点消息路由

### 📊 性能目标

| 阶段 | 并发连接数 | 消息吞吐量 | 延迟 | 内存使用 |
|------|-----------|-----------|------|----------|
| 第一阶段 | 1,000 | 10K msg/s | <10ms | 适中 |
| 第二阶段 | 10,000 | 100K msg/s | <5ms | 优化 |
| 第三阶段 | 100,000+ | 1M+ msg/s | <1ms | 极优 |

### 🎯 当前状态
- ✅ 基础功能实现完成
- 🔄 **正在进行**: 第一阶段紧急修复
- ⏳ 计划中: 第二阶段性能优化
- ⏳ 规划中: 第三阶段架构重构