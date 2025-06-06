pub mod adapter;
pub mod protocol;
pub mod protocol_adapter;

// 重新导出适配器相关类型
pub use adapter::{ProtocolAdapter, AdapterStats, ProtocolConfig, ConfigError, TcpConfig, WebSocketConfig, QuicConfig};

// 重新导出协议相关类型
pub use protocol::{Connection, Server, ProtocolFactory, ProtocolRegistry, BoxFuture, ProtocolSet, StandardProtocols, PluginManager};

// 重新导出协议适配器
pub use protocol_adapter::{ProtocolConnectionAdapter, EmptyConfig}; 