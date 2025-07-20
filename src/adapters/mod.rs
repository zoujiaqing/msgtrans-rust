/// Protocol adapter implementation module
/// 
/// This module contains specific adapter implementations for various transport protocols

pub mod tcp;
pub mod websocket;
pub mod quic;
pub mod factories;

// Only export standard APIs that users really need (remove Builder classes)
pub use tcp::{TcpAdapter, TcpError};
pub use websocket::{WebSocketAdapter, WebSocketError};
pub use quic::{QuicAdapter, QuicError};

// Export factory and connection types
pub use factories::{create_standard_registry, TcpFactory, TcpServerWrapper};
pub use factories::{WebSocketFactory, WebSocketConnection, WebSocketServerWrapper};
pub use factories::{QuicFactory, QuicConnection, QuicServerWrapper}; 