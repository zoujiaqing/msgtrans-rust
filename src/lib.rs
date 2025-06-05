/// msgtrans - 统一多协议传输库
/// 
/// 这是一个现代的、高性能的Rust传输库，提供TCP、WebSocket和QUIC协议的统一接口。
/// 基于Actor模式设计，完全消除回调地狱，提供类型安全的事件驱动API。

// 核心模块
pub mod packet;
pub mod compression;

// 统一架构模块
pub mod unified;

// 重新导出核心类型，用户只需要导入这些
pub use unified::{
    // 数据包类型
    UnifiedPacket, PacketType, PacketError,
    
    // 传输核心API
    Transport, TransportBuilder, 
    ConnectionManager, ServerManager,
    
    // 事件系统
    TransportEvent, EventStream,
    
    // 错误处理
    TransportError,
    
    // 配置系统
    TransportConfig, GlobalConfig,
    
    // 连接信息
    ConnectionInfo, TransportStats,
};

// 便捷的类型别名
pub type Result<T> = std::result::Result<T, TransportError>;
pub type Packet = UnifiedPacket;