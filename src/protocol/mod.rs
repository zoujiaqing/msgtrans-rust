/// Protocol abstraction layer module
/// 
/// Provides abstractions for protocol configuration, adapters, and factories

pub mod adapter;
pub mod protocol;
pub mod client_config;
pub mod server_config;
pub mod protocol_adapter;

// Re-export core types
pub use adapter::{ProtocolAdapter, AdapterStats};
pub use protocol::{ProtocolFactory, ProtocolRegistry, BoxFuture, ProtocolSet, StandardProtocols, PluginManager};

// Re-export configuration types
pub use client_config::{TcpClientConfig, WebSocketClientConfig, QuicClientConfig, RetryConfig};
pub use server_config::{TcpServerConfig, WebSocketServerConfig, QuicServerConfig};

// Re-export adapter configuration
pub use adapter::{
    ProtocolConfig, ConfigError, 
    DynProtocolConfig, DynServerConfig, DynClientConfig,
    ServerConfig, ClientConfig
}; 