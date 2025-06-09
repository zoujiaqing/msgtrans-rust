# 传输层重新设计：分离式Builder与流式API

## 🎯 设计目标

重新设计msgtrans-rust的传输层架构，解决以下问题：
1. 配置对象不应包含行为方法（违反单一职责原则）
2. Builder应该明确区分客户端和服务端用途
3. 提供流式API：`transport.with_protocol(config).connect()`
4. 支持动态协议扩展，无硬编码协议名称

## 🏗️ 架构概览

```
TransportClientBuilder -> ClientTransport -> ProtocolConnectionBuilder -> connect()
TransportServerBuilder -> ServerTransport -> ProtocolServerBuilder -> serve()
```

## 📋 详细设计

### 1. 分离式Builder设计

#### 客户端构建器
```rust
pub struct TransportClientBuilder {
    connect_timeout: Duration,
    pool_config: ConnectionPoolConfig,
    retry_config: RetryConfig,
    load_balancer: Option<LoadBalancerConfig>,
    circuit_breaker: Option<CircuitBreakerConfig>,
    connection_monitoring: bool,
}

impl TransportClientBuilder {
    pub fn new() -> Self { /* ... */ }
    
    // 客户端专用配置
    pub fn connect_timeout(mut self, timeout: Duration) -> Self { /* ... */ }
    pub fn connection_pool(mut self, config: ConnectionPoolConfig) -> Self { /* ... */ }
    pub fn retry_strategy(mut self, config: RetryConfig) -> Self { /* ... */ }
    pub fn load_balancer(mut self, config: LoadBalancerConfig) -> Self { /* ... */ }
    pub fn circuit_breaker(mut self, config: CircuitBreakerConfig) -> Self { /* ... */ }
    
    pub async fn build(self) -> Result<ClientTransport, TransportError> { /* ... */ }
}
```

#### 服务端构建器
```rust
pub struct TransportServerBuilder {
    bind_timeout: Duration,
    max_connections: usize,
    acceptor_config: AcceptorConfig,
    rate_limiter: Option<RateLimiterConfig>,
    middleware_stack: Vec<Box<dyn ServerMiddleware>>,
    graceful_shutdown: Option<Duration>,
}

impl TransportServerBuilder {
    pub fn new() -> Self { /* ... */ }
    
    // 服务端专用配置
    pub fn bind_timeout(mut self, timeout: Duration) -> Self { /* ... */ }
    pub fn max_connections(mut self, max: usize) -> Self { /* ... */ }
    pub fn acceptor_threads(mut self, threads: usize) -> Self { /* ... */ }
    pub fn rate_limiter(mut self, config: RateLimiterConfig) -> Self { /* ... */ }
    pub fn with_middleware<M: ServerMiddleware + 'static>(mut self, middleware: M) -> Self { /* ... */ }
    
    pub async fn build(self) -> Result<ServerTransport, TransportError> { /* ... */ }
}
```

### 2. 传输层实现

#### 客户端传输层
```rust
pub struct ClientTransport {
    inner: Transport,
    pool_config: ConnectionPoolConfig,
    retry_config: RetryConfig,
    // 客户端专用状态...
}

impl ClientTransport {
    /// 流式API入口
    pub fn with_protocol<C>(&self, config: C) -> ProtocolConnectionBuilder<'_, C>
    where
        C: ProtocolConfig + ConnectableConfig,
    {
        ProtocolConnectionBuilder::new(self, config)
    }
    
    // 客户端专用功能
    pub async fn connect_multiple<C>(&self, configs: Vec<C>) -> Result<Vec<SessionId>, TransportError> { /* ... */ }
    pub async fn create_connection_pool<C>(&self, config: C, pool_size: usize) -> Result<ClientConnectionPool<C>, TransportError> { /* ... */ }
}
```

#### 服务端传输层
```rust
pub struct ServerTransport {
    inner: Transport,
    servers: Arc<tokio::sync::Mutex<HashMap<String, Box<dyn Server>>>>,
    acceptor_config: AcceptorConfig,
    // 服务端专用状态...
}

impl ServerTransport {
    /// 流式API入口
    pub fn with_protocol<C>(&self, config: C) -> ProtocolServerBuilder<'_, C>
    where
        C: ProtocolConfig + ServerConfig,
    {
        ProtocolServerBuilder::new(self, config)
    }
    
    // 服务端专用功能
    pub async fn serve_multiple<C>(&self, configs: Vec<C>) -> Result<Vec<String>, TransportError> { /* ... */ }
    pub async fn stop_server(&self, server_id: &str) -> Result<(), TransportError> { /* ... */ }
    pub async fn stop_all(&self) -> Result<(), TransportError> { /* ... */ }
}
```

### 3. 流式API实现

#### 客户端连接构建器
```rust
pub struct ProtocolConnectionBuilder<'t, C> {
    transport: &'t ClientTransport,
    config: C,
    connection_options: ConnectionOptions,
}

impl<'t, C> ProtocolConnectionBuilder<'t, C>
where
    C: ProtocolConfig + ConnectableConfig,
{
    pub fn with_timeout(mut self, timeout: Duration) -> Self { /* ... */ }
    pub fn with_retry(mut self, max_retries: usize) -> Self { /* ... */ }
    pub fn with_priority(mut self, priority: ConnectionPriority) -> Self { /* ... */ }
    
    /// 核心连接方法
    pub async fn connect(self) -> Result<SessionId, TransportError> {
        // 配置验证
        self.config.validate()
            .map_err(|e| TransportError::config_error("protocol", format!("Config validation failed: {:?}", e)))?;
        
        // 应用超时、重试等选项
        if let Some(timeout) = self.connection_options.timeout {
            tokio::time::timeout(timeout, self.connect_inner()).await
                .map_err(|_| TransportError::connection_error("Connection timeout", true))?
        } else {
            self.connect_inner().await
        }
    }
    
    async fn connect_inner(self) -> Result<SessionId, TransportError> {
        // 重试逻辑
        let mut last_error = None;
        for attempt in 0..=self.connection_options.max_retries {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
            }
            
            match self.config.connect(&self.transport.inner).await {
                Ok(session_id) => return Ok(session_id),
                Err(e) => last_error = Some(e),
            }
        }
        
        Err(last_error.unwrap_or_else(|| 
            TransportError::connection_error("All retry attempts failed", true)
        ))
    }
}
```

#### 服务端构建器
```rust
pub struct ProtocolServerBuilder<'t, C> {
    transport: &'t ServerTransport,
    config: C,
    server_options: ServerOptions,
}

impl<'t, C> ProtocolServerBuilder<'t, C>
where
    C: ProtocolConfig + ServerConfig,
{
    pub fn with_name(mut self, name: String) -> Self { /* ... */ }
    pub fn with_max_connections(mut self, max: usize) -> Self { /* ... */ }
    
    /// 核心服务方法
    pub async fn serve(self) -> Result<String, TransportError> {
        self.config.validate()
            .map_err(|e| TransportError::config_error("protocol", format!("Config validation failed: {:?}", e)))?;
        
        let server = self.config.build_server().await?;
        let server_id = self.server_options.name
            .unwrap_or_else(|| format!("server-{}", uuid::Uuid::new_v4()));
        
        let mut servers = self.transport.servers.lock().await;
        servers.insert(server_id.clone(), server);
        
        Ok(server_id)
    }
}
```

### 4. 纯配置设计

#### 协议配置trait
```rust
/// 纯配置trait - 只管配置，不管行为
pub trait ProtocolConfig: Send + Sync + Clone + Debug + 'static {
    fn validate(&self) -> Result<(), ConfigError>;
    fn protocol_name(&self) -> &'static str;
    fn default_config() -> Self;
    fn merge(self, other: Self) -> Self;
}

/// 可连接配置trait - 分离的行为
#[async_trait]
pub trait ConnectableConfig: Send + Sync {
    async fn connect(&self, transport: &Transport) -> Result<SessionId, TransportError>;
}

/// 服务器配置trait - 分离的行为
pub trait ServerConfig: Send + Sync + 'static {
    type Server: crate::protocol::Server;
    
    fn validate(&self) -> Result<(), TransportError>;
    fn build_server(&self) -> impl std::future::Future<Output = Result<Self::Server, TransportError>> + Send;
    fn protocol_name(&self) -> &'static str;
}
```

## 📖 使用示例

### 客户端使用
```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 🔌 构建客户端传输层
    let client = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .connection_pool(ConnectionPoolConfig {
            max_size: 100,
            idle_timeout: Duration::from_secs(300),
        })
        .retry_strategy(RetryConfig::exponential_backoff(3, Duration::from_millis(100)))
        .circuit_breaker(CircuitBreakerConfig {
            failure_threshold: 10,
            timeout: Duration::from_secs(60),
        })
        .build()
        .await?;
    
    // 🎯 流式API连接 - 简洁优雅
    let tcp_config = TcpConfig {
        bind_address: "127.0.0.1:8080".parse()?,
        nodelay: true,
        keepalive: Some(Duration::from_secs(60)),
        ..Default::default()
    };
    
    let session_id = client
        .with_protocol(tcp_config)
        .with_timeout(Duration::from_secs(10))
        .with_retry(3)
        .with_priority(ConnectionPriority::High)
        .connect()
        .await?;
    
    println!("连接建立: {:?}", session_id);
    
    // 发送消息
    let packet = Packet::new(b"Hello, World!".to_vec());
    client.send(session_id, packet).await?;
    
    Ok(())
}
```

### 服务端使用
```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 🚀 构建服务端传输层
    let server = TransportServerBuilder::new()
        .bind_timeout(Duration::from_secs(3))
        .max_connections(5000)
        .acceptor_threads(8)
        .rate_limiter(RateLimiterConfig {
            requests_per_second: 1000,
            burst_size: 100,
        })
        .with_middleware(LoggingMiddleware::new())
        .with_middleware(AuthMiddleware::new())
        .graceful_shutdown(Some(Duration::from_secs(30)))
        .build()
        .await?;
    
    // 🎯 流式API服务 - 简洁优雅
    let tcp_config = TcpConfig {
        bind_address: "0.0.0.0:8080".parse()?,
        ..Default::default()
    };
    
    let server_id = server
        .with_protocol(tcp_config)
        .with_name("tcp-echo-server".to_string())
        .with_max_connections(1000)
        .serve()
        .await?;
    
    println!("服务器启动: {}", server_id);
    
    // 同时启动多个协议服务器
    let quic_config = QuicConfig {
        bind_address: "0.0.0.0:8443".parse()?,
        ..Default::default()
    };
    
    let server_ids = server.serve_multiple(vec![quic_config]).await?;
    println!("多协议服务器启动: {:?}", server_ids);
    
    // 优雅关闭
    tokio::signal::ctrl_c().await?;
    server.stop_all().await?;
    
    Ok(())
}
```

### 自定义协议支持
```rust
// 定义自定义协议配置
#[derive(Debug, Clone)]
pub struct MyCustomConfig {
    pub endpoint: String,
    pub auth_token: String,
    pub compression: bool,
}

impl ProtocolConfig for MyCustomConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.endpoint.is_empty() {
            return Err(ConfigError::MissingRequiredField {
                field: "endpoint".to_string(),
                suggestion: "provide a valid endpoint".to_string(),
            });
        }
        Ok(())
    }
    
    fn protocol_name(&self) -> &'static str { "my-custom" }
    fn default_config() -> Self { /* ... */ }
    fn merge(self, other: Self) -> Self { /* ... */ }
}

#[async_trait]
impl ConnectableConfig for MyCustomConfig {
    async fn connect(&self, transport: &Transport) -> Result<SessionId, TransportError> {
        // 自定义连接逻辑
        let adapter = MyCustomAdapter::connect(&self.endpoint, &self.auth_token).await?;
        transport.add_connection(adapter).await
    }
}

// 使用自定义协议
let custom_config = MyCustomConfig {
    endpoint: "https://my-api.com".to_string(),
    auth_token: "secret".to_string(),
    compression: true,
};

let session_id = client
    .with_protocol(custom_config)
    .connect()
    .await?;
```

## 🎉 设计优势

1. **意图明确**：从Builder类型就知道构建客户端还是服务端
2. **职责分离**：配置只管配置，行为分离到trait中
3. **类型安全**：编译时确保正确的配置类型和使用方式
4. **API优雅**：流式API`transport.with_protocol(config).connect()`简洁直观
5. **易于扩展**：新协议只需实现trait，无需修改核心代码
6. **专业配置**：每个Builder只有相关的配置选项
7. **向下兼容**：可以保留原有API作为便利方法

## 🔄 实现步骤

1. ✅ 创建设计文档
2. 🔧 实现分离式Builder（TransportClientBuilder/TransportServerBuilder）
3. 🔧 实现ClientTransport和ServerTransport
4. 🔧 实现流式API（ProtocolConnectionBuilder/ProtocolServerBuilder）
5. 🔧 重构现有配置类，分离配置和行为
6. 🔧 更新示例和文档
7. 🔧 添加测试用例

## 📝 向后兼容性

为了平滑迁移，保留原有API作为便利方法：
```rust
// 原有API继续工作
impl Transport {
    pub fn builder() -> TransportBuilder {
        TransportBuilder::Client(TransportClientBuilder::new())
    }
}

pub enum TransportBuilder {
    Client(TransportClientBuilder),
    Server(TransportServerBuilder),
}
```

这样现有代码可以继续工作，同时新代码可以使用更优雅的分离式API。 