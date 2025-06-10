pub mod adapter;
pub mod protocol;
pub mod server_config;
pub mod client_config;
pub mod protocol_adapter;

// 重新导出适配器相关类型（移除旧配置）
pub use adapter::{
    ProtocolAdapter, ProtocolConfig,
    ConfigError,
    ServerConfig, ClientConfig,  // 只保留trait
    AdapterStats
};

// 重新导出协议相关类型
pub use protocol::{Connection, Server, ProtocolFactory, ProtocolRegistry, BoxFuture, ProtocolSet, StandardProtocols, PluginManager};

// 重新导出协议适配器
pub use protocol_adapter::{ProtocolConnectionAdapter, EmptyConfig};

// 重新导出分离的配置类型（这是唯一的公开配置API）
pub use server_config::{TcpServerConfig, WebSocketServerConfig, QuicServerConfig};
pub use client_config::{TcpClientConfig, WebSocketClientConfig, QuicClientConfig, RetryConfig}; 