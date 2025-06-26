/// 连接工厂：统一管理不同类型的连接创建
/// 
/// 支持渐进式从传统连接迁移到无锁连接

use std::sync::Arc;
use tokio::task::JoinHandle;
use crate::{
    Connection, SessionId, TransportError,
    transport::LockFreeConnection,
};

/// 🚀 第三阶段终极简化：连接创建结果
pub struct ConnectionResult {
    /// 连接实例（统一无锁连接）
    pub connection: Box<dyn Connection>,
    /// 工作器句柄
    pub worker_handle: Option<JoinHandle<()>>,
}

/// 连接性能指标
#[derive(Debug, Clone)]
pub struct ConnectionMetrics {
    /// 创建时间
    pub creation_time: std::time::Duration,
    /// 缓冲区大小
    pub buffer_size: usize,
}

/// 🚀 第三阶段：简化连接配置
#[derive(Clone, Debug)]
pub struct ConnectionConfig {
    /// 缓冲区大小
    pub buffer_size: usize,
    /// 是否启用性能监控
    pub enable_metrics: bool,
    /// 是否启用智能优化（自动根据CPU调整缓冲区）
    pub auto_optimize: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1500,
            enable_metrics: true,
            auto_optimize: true, // 默认启用智能优化
        }
    }
}

impl ConnectionConfig {
    /// 高性能配置
    pub fn high_performance() -> Self {
        Self {
            buffer_size: 2500,
            enable_metrics: true,
            auto_optimize: false, // 手动指定高性能参数
        }
    }
    
    /// 智能优化配置
    pub fn auto_optimized() -> Self {
        Self {
            buffer_size: 1000, // 基础值，会被智能优化覆盖
            enable_metrics: true,
            auto_optimize: true,
        }
    }
    
    /// 静默配置（关闭监控）
    pub fn silent() -> Self {
        Self {
            buffer_size: 1000,
            enable_metrics: false,
            auto_optimize: false,
        }
    }
    
    /// 自定义缓冲区大小
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self.auto_optimize = false; // 手动指定则关闭自动优化
        self
    }
    
    /// 启用/禁用智能优化
    pub fn with_auto_optimize(mut self, enabled: bool) -> Self {
        self.auto_optimize = enabled;
        self
    }
}

/// 🚀 第三阶段终极简化：连接工厂
pub struct ConnectionFactory;

impl ConnectionFactory {
    /// 创建无锁连接（终极简化API）
    pub fn create_connection(
        adapter: Box<dyn Connection>,
        session_id: SessionId,
    ) -> Result<ConnectionResult, TransportError> {
        Self::create_connection_with_config(adapter, session_id, ConnectionConfig::default())
    }
    
    /// 创建带配置的无锁连接
    pub fn create_connection_with_config(
        adapter: Box<dyn Connection>,
        session_id: SessionId,
        config: ConnectionConfig,
    ) -> Result<ConnectionResult, TransportError> {
        let start_time = std::time::Instant::now();
        
        // 🚀 第三阶段：智能优化缓冲区大小
        let final_buffer_size = if config.auto_optimize {
            Self::optimize_buffer_size(config.buffer_size)
        } else {
            config.buffer_size
        };
        
        tracing::info!(
            "🚀 第三阶段：创建无锁连接 (会话: {}, 缓冲区: {})",
            session_id,
            final_buffer_size
        );
        
        // 创建无锁连接
        let (lockfree_conn, worker_handle) = LockFreeConnection::new(
            adapter,
            session_id,
            final_buffer_size,
        );
        
        let result = ConnectionResult {
            connection: Box::new(lockfree_conn),
            worker_handle: Some(worker_handle),
        };
        
        // 记录性能指标
        if config.enable_metrics {
            let creation_time = start_time.elapsed();
            let metrics = ConnectionMetrics {
                creation_time,
                buffer_size: final_buffer_size,
            };
            
            Self::record_metrics(session_id, metrics);
        }
        
        Ok(result)
    }
    
    /// 🚀 第三阶段：智能缓冲区优化
    fn optimize_buffer_size(base_size: usize) -> usize {
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        
        let optimized_size = match cpu_count {
            1..=2 => base_size.max(1000),      // 单核/双核：保守配置
            3..=4 => base_size.max(1500),      // 四核：平衡配置
            5..=8 => base_size.max(2000),      // 八核：高性能配置
            _ => base_size.max(2500),          // 多核：最高性能配置
        };
        
        tracing::debug!(
            "🎯 第三阶段智能优化：CPU{}核，缓冲区 {} → {}",
            cpu_count,
            base_size,
            optimized_size
        );
        
        optimized_size
    }
    
    /// 🚀 第三阶段：简化环境变量处理
    pub fn from_env() -> ConnectionConfig {
        if let Ok(buffer_size) = std::env::var("MSGTRANS_BUFFER_SIZE") {
            if let Ok(size) = buffer_size.parse::<usize>() {
                tracing::info!("🔧 环境变量指定缓冲区大小: {}", size);
                return ConnectionConfig::default().with_buffer_size(size);
            }
        }
        
        if let Ok(conn_type) = std::env::var("MSGTRANS_CONNECTION_TYPE") {
            match conn_type.to_lowercase().as_str() {
                "traditional" => {
                    tracing::warn!(
                        "🚨 第三阶段通知：传统连接已移除，使用智能优化无锁连接"
                    );
                    return ConnectionConfig::auto_optimized();
                }
                "auto" => {
                    tracing::info!("🔧 环境变量指定智能优化");
                    return ConnectionConfig::auto_optimized();
                }
                "high_performance" => {
                    tracing::info!("🔧 环境变量指定高性能模式");
                    return ConnectionConfig::high_performance();
                }
                _ => {
                    tracing::warn!("🚨 未知的连接配置: {}, 使用默认配置", conn_type);
                }
            }
        }
        
        ConnectionConfig::default()
    }
    
    /// 记录性能指标
    fn record_metrics(session_id: SessionId, metrics: ConnectionMetrics) {
        tracing::info!(
            "📊 连接创建指标 - 会话: {}, 耗时: {:?}, 缓冲区: {}",
            session_id,
            metrics.creation_time,
            metrics.buffer_size
        );
    }
    
    /// 🚀 第三阶段：获取推荐配置
    pub fn recommend_config() -> ConnectionConfig {
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        
        match cpu_count {
            1..=2 => {
                tracing::debug!("💡 第三阶段：{}核CPU，推荐智能优化模式", cpu_count);
                ConnectionConfig::auto_optimized()
            }
            _ => {
                tracing::debug!("💡 第三阶段：{}核CPU，推荐高性能模式", cpu_count);
                ConnectionConfig::high_performance()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_auto_detect_from_env() {
        // 设置环境变量
        std::env::set_var("MSGTRANS_CONNECTION_TYPE", "lockfree");
        let detected = ConnectionFactory::from_env();
        assert_eq!(detected.buffer_size, 1000);
        
        // 第三阶段：传统连接自动转换为无锁连接
        std::env::set_var("MSGTRANS_CONNECTION_TYPE", "traditional");
        let detected = ConnectionFactory::from_env();
        assert_eq!(detected.buffer_size, 1000);
        
        std::env::set_var("MSGTRANS_CONNECTION_TYPE", "auto");
        let detected = ConnectionFactory::from_env();
        assert_eq!(detected.buffer_size, 1000);
        
        // 清理环境变量
        std::env::remove_var("MSGTRANS_CONNECTION_TYPE");
    }
    
    #[test]
    fn test_recommend_config() {
        let recommended = ConnectionFactory::recommend_config();
        
        // 应该推荐LockFree或Auto（取决于CPU核心数）
        assert!(matches!(recommended.buffer_size, 2500 | 1000));
    }
    
    #[test]
    fn test_config_presets() {
        let high_perf = ConnectionConfig::high_performance();
        assert_eq!(high_perf.buffer_size, 2500);
        assert!(high_perf.enable_metrics);
        
        let auto_optimized = ConnectionConfig::auto_optimized();
        assert_eq!(auto_optimized.buffer_size, 1000);
        assert!(auto_optimized.enable_metrics);
        
        let silent = ConnectionConfig::silent();
        assert_eq!(silent.buffer_size, 1000);
        assert!(!silent.enable_metrics);
    }
} 