/// 协议抽象层模块
/// 
/// 提供协议配置、适配器和工厂等抽象

pub mod adapter;
pub mod protocol;
pub mod client_config;
pub mod server_config;
pub mod protocol_adapter;

// 重新导出核心类型
pub use adapter::{ProtocolAdapter, AdapterStats};
pub use protocol::{ProtocolFactory, ProtocolRegistry, BoxFuture, ProtocolSet, StandardProtocols, PluginManager};

// 重新导出配置类型
pub use client_config::{TcpClientConfig, WebSocketClientConfig, QuicClientConfig, RetryConfig};
pub use server_config::{TcpServerConfig, WebSocketServerConfig, QuicServerConfig};

// 重新导出适配器配置
pub use adapter::{
    ProtocolConfig, ConfigError, 
    DynProtocolConfig, DynServerConfig, DynClientConfig,
    ServerConfig, ClientConfig
}; 