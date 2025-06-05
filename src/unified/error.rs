use std::time::Duration;
use super::{SessionId, TransportEvent};

/// 统一传输错误类型
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// 连接相关错误
    #[error("Connection error: {0}")]
    Connection(String),
    
    /// 协议相关错误
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    /// 配置相关错误
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    /// IO错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// 序列化错误
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// 会话相关错误
    #[error("Session not found")]
    SessionNotFound,
    
    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),
    
    #[error("Protocol configuration error: {0}")]
    ProtocolConfiguration(String),
    
    /// 无效会话
    #[error("Invalid session")]
    InvalidSession,
    
    /// 通道相关错误
    #[error("Channel closed")]
    ChannelClosed,
    
    /// 通道滞后
    #[error("Channel lagged")]
    ChannelLagged,
    
    /// Actor通信错误
    #[error("Actor communication error")]
    ActorCommunicationError,
    
    /// 广播失败
    #[error("Broadcast failed to {} sessions", .0.len())]
    BroadcastFailed(Vec<(SessionId, TransportError)>),
    
    /// 超时错误
    #[error("Operation timeout")]
    Timeout,
    
    /// 认证错误
    #[error("Authentication failed: {0}")]
    Authentication(String),
    
    /// 权限错误
    #[error("Permission denied: {0}")]
    Permission(String),
}

impl Clone for TransportError {
    fn clone(&self) -> Self {
        match self {
            TransportError::Connection(s) => TransportError::Connection(s.clone()),
            TransportError::Protocol(s) => TransportError::Protocol(s.clone()),
            TransportError::Configuration(s) => TransportError::Configuration(s.clone()),
            TransportError::Io(e) => TransportError::Io(std::io::Error::new(e.kind(), e.to_string())),
            TransportError::Serialization(s) => TransportError::Serialization(s.clone()),
            TransportError::SessionNotFound => TransportError::SessionNotFound,
            TransportError::UnsupportedProtocol(s) => TransportError::UnsupportedProtocol(s.clone()),
            TransportError::ProtocolConfiguration(s) => TransportError::ProtocolConfiguration(s.clone()),
            TransportError::InvalidSession => TransportError::InvalidSession,
            TransportError::ChannelClosed => TransportError::ChannelClosed,
            TransportError::ChannelLagged => TransportError::ChannelLagged,
            TransportError::ActorCommunicationError => TransportError::ActorCommunicationError,
            TransportError::BroadcastFailed(v) => TransportError::BroadcastFailed(v.clone()),
            TransportError::Timeout => TransportError::Timeout,
            TransportError::Authentication(s) => TransportError::Authentication(s.clone()),
            TransportError::Permission(s) => TransportError::Permission(s.clone()),
        }
    }
}

impl From<TransportError> for TransportEvent {
    fn from(error: TransportError) -> Self {
        TransportEvent::TransportError {
            session_id: None,
            error,
        }
    }
}

/// 错误恢复策略
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// 重试
    Retry {
        max_attempts: u32,
        backoff: Duration,
    },
    /// 重连
    Reconnect {
        delay: Duration,
    },
    /// 降级
    Fallback {
        alternative: String,
    },
    /// 中止
    Abort,
    /// 忽略错误继续
    Ignore,
}

/// 错误处理器trait
#[async_trait::async_trait]
pub trait ErrorHandler: Send + Sync {
    /// 处理错误并返回恢复策略
    async fn handle_error(&self, error: &TransportError) -> RecoveryStrategy;
    
    /// 恢复成功时的回调
    async fn on_recovery_success(&self, error: &TransportError);
    
    /// 恢复失败时的回调
    async fn on_recovery_failure(&self, error: &TransportError, strategy: &RecoveryStrategy);
}

/// 默认错误处理器
#[derive(Debug, Default)]
pub struct DefaultErrorHandler {
    pub max_retry_attempts: u32,
    pub retry_backoff: Duration,
    pub reconnect_delay: Duration,
}

impl DefaultErrorHandler {
    pub fn new() -> Self {
        Self {
            max_retry_attempts: 3,
            retry_backoff: Duration::from_millis(100),
            reconnect_delay: Duration::from_secs(1),
        }
    }
    
    pub fn with_max_retries(mut self, max_attempts: u32) -> Self {
        self.max_retry_attempts = max_attempts;
        self
    }
    
    pub fn with_retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self
    }
    
    pub fn with_reconnect_delay(mut self, delay: Duration) -> Self {
        self.reconnect_delay = delay;
        self
    }
}

#[async_trait::async_trait]
impl ErrorHandler for DefaultErrorHandler {
    async fn handle_error(&self, error: &TransportError) -> RecoveryStrategy {
        match error {
            // 网络相关错误 - 尝试重连
            TransportError::Io(_) |
            TransportError::Connection(_) => {
                RecoveryStrategy::Reconnect {
                    delay: self.reconnect_delay,
                }
            }
            
            // 协议错误 - 尝试重试
            TransportError::Protocol(_) |
            TransportError::Serialization(_) => {
                RecoveryStrategy::Retry {
                    max_attempts: self.max_retry_attempts,
                    backoff: self.retry_backoff,
                }
            }
            
            // 超时错误 - 重试
            TransportError::Timeout => {
                RecoveryStrategy::Retry {
                    max_attempts: self.max_retry_attempts,
                    backoff: self.retry_backoff,
                }
            }
            
            // 配置错误 - 中止
            TransportError::Configuration(_) |
            TransportError::UnsupportedProtocol(_) |
            TransportError::ProtocolConfiguration(_) |
            TransportError::Authentication(_) |
            TransportError::Permission(_) => {
                RecoveryStrategy::Abort
            }
            
            // 会话相关错误 - 忽略
            TransportError::SessionNotFound |
            TransportError::InvalidSession => {
                RecoveryStrategy::Ignore
            }
            
            // 通道错误 - 重连
            TransportError::ChannelClosed |
            TransportError::ChannelLagged |
            TransportError::ActorCommunicationError => {
                RecoveryStrategy::Reconnect {
                    delay: self.reconnect_delay,
                }
            }
            
            // 广播失败 - 忽略
            TransportError::BroadcastFailed(_) => {
                RecoveryStrategy::Ignore
            }
        }
    }
    
    async fn on_recovery_success(&self, error: &TransportError) {
        tracing::info!("Recovered from error: {:?}", error);
    }
    
    async fn on_recovery_failure(&self, error: &TransportError, strategy: &RecoveryStrategy) {
        tracing::error!("Failed to recover from error: {:?} using strategy: {:?}", error, strategy);
    }
}

/// 错误分类器
pub struct ErrorClassifier;

impl ErrorClassifier {
    /// 判断错误是否可重试
    pub fn is_retryable(error: &TransportError) -> bool {
        matches!(error,
            TransportError::Protocol(_) |
            TransportError::Serialization(_) |
            TransportError::Timeout |
            TransportError::ChannelLagged
        )
    }
    
    /// 判断错误是否需要重连
    pub fn needs_reconnection(error: &TransportError) -> bool {
        matches!(error,
            TransportError::Io(_) |
            TransportError::Connection(_) |
            TransportError::ChannelClosed |
            TransportError::ActorCommunicationError
        )
    }
    
    /// 判断错误是否是致命的
    pub fn is_fatal(error: &TransportError) -> bool {
        matches!(error,
            TransportError::Configuration(_) |
            TransportError::UnsupportedProtocol(_) |
            TransportError::ProtocolConfiguration(_) |
            TransportError::Authentication(_) |
            TransportError::Permission(_)
        )
    }
    
    /// 获取错误的严重程度
    pub fn severity(error: &TransportError) -> ErrorSeverity {
        match error {
            TransportError::Configuration(_) |
            TransportError::UnsupportedProtocol(_) |
            TransportError::ProtocolConfiguration(_) |
            TransportError::Authentication(_) |
            TransportError::Permission(_) => ErrorSeverity::Critical,
            
            TransportError::Io(_) |
            TransportError::Connection(_) |
            TransportError::ChannelClosed |
            TransportError::ActorCommunicationError => ErrorSeverity::High,
            
            TransportError::Protocol(_) |
            TransportError::Serialization(_) |
            TransportError::Timeout |
            TransportError::ChannelLagged => ErrorSeverity::Medium,
            
            TransportError::SessionNotFound |
            TransportError::InvalidSession |
            TransportError::BroadcastFailed(_) => ErrorSeverity::Low,
        }
    }
}

/// 错误严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// 错误统计信息
#[derive(Debug, Clone)]
pub struct ErrorStats {
    pub total_errors: u64,
    pub retries: u64,
    pub reconnections: u64,
    pub fatal_errors: u64,
    pub last_error: Option<TransportError>,
}

impl Default for ErrorStats {
    fn default() -> Self {
        Self {
            total_errors: 0,
            retries: 0,
            reconnections: 0,
            fatal_errors: 0,
            last_error: None,
        }
    }
}

impl ErrorStats {
    pub fn record_error(&mut self, error: TransportError) {
        self.total_errors += 1;
        
        if ErrorClassifier::is_retryable(&error) {
            self.retries += 1;
        }
        
        if ErrorClassifier::needs_reconnection(&error) {
            self.reconnections += 1;
        }
        
        if ErrorClassifier::is_fatal(&error) {
            self.fatal_errors += 1;
        }
        
        self.last_error = Some(error);
    }
    
    pub fn clear(&mut self) {
        *self = Self::default();
    }
} 