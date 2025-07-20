#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(private_bounds)]
#![allow(private_interfaces)]
#![allow(async_fn_in_trait)]
#![allow(unused_must_use)]
#![allow(non_upper_case_globals)]

/// msgtrans - Unified multi-protocol transport library
/// 
/// This is a modern, high-performance Rust transport library providing unified interfaces for TCP, WebSocket and QUIC protocols.
/// Designed based on Actor pattern, completely eliminates callback hell and provides type-safe event-driven API.

// Transport layer
pub mod transport;

// Protocol adapters
pub mod adapters;

// Protocol abstraction
pub mod protocol;

// Core types
pub mod packet;
pub mod event;
pub mod error;
pub mod command;
pub mod stream;

// New modules
pub mod connection;
pub mod plugin;

// Type definitions
pub type PacketId = u32;

/// Type-safe wrapper for session ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(u64);

impl SessionId {
    /// Create new session ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// Get raw ID value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
    
    /// Generate next session ID
    pub fn next(&self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session-{}", self.0)
    }
}

impl From<u64> for SessionId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<SessionId> for u64 {
    fn from(session_id: SessionId) -> Self {
        session_id.0
    }
}

// Re-export core types
pub use packet::{Packet, PacketType, PacketError};
pub use event::{TransportEvent, ClientEvent, TcpEvent, WebSocketEvent, QuicEvent};
pub use error::{TransportError, CloseReason};
pub use command::{TransportCommand, TransportStats, ConnectionInfo};
pub use stream::{EventStream, PacketStream, ClientEventStream};

pub use transport::{
    TransportConfig, ExpertConfig, SmartPoolConfig, PerformanceConfig,
    // Core transport types
    Transport, TransportServer,
    // Builders
    TransportClientBuilder, TransportServerBuilder, 
    // Client and server
    TransportClient,
    // Advanced configuration
    ConnectionPoolConfig, RetryConfig, LoadBalancerConfig, CircuitBreakerConfig,
    AcceptorConfig, BackpressureStrategy, RateLimiterConfig,
    // Connection pool and memory management
    ConnectionPool, MemoryPool, MemoryStats, MemoryStatsSnapshot,
    // High-performance components
    Actor, ActorManager, ProtocolAdapter, ProtocolStats,
    // LockFree base components
    LockFreeHashMap, LockFreeQueue, LockFreeCounter,
};

pub use protocol::{TcpClientConfig, TcpServerConfig, WebSocketClientConfig, WebSocketServerConfig, QuicClientConfig, QuicServerConfig, ServerConfig, ClientConfig};
// Re-export new abstractions
pub use connection::{Connection, Server, ConnectionFactory};
pub use plugin::{ProtocolPlugin, PluginManager, PluginInfo};

// Re-export tokio for user convenience
pub use tokio;

// Convenient type aliases
pub type Result<T> = std::result::Result<T, TransportError>;