/// 专家级配置 - 高级Builder配置项
/// 
/// 为高级用户提供精细化的性能调优、监控和韧性配置选项

use std::time::Duration;
use serde::{Serialize, Deserialize};

use super::pool::ExpansionStrategy;
use crate::error::TransportError;

/// 智能连接池专家配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartPoolConfig {
    /// 初始连接池大小
    pub initial_size: usize,
    /// 最大连接池大小
    pub max_size: usize,
    /// 扩展阈值 (0.0-1.0, 使用率触发扩展)
    pub expansion_threshold: f64,
    /// 收缩阈值 (0.0-1.0, 使用率触发收缩)
    pub shrink_threshold: f64,
    /// 扩展冷却时间
    pub expansion_cooldown: Duration,
    /// 收缩冷却时间
    pub shrink_cooldown: Duration,
    /// 最小保留连接数
    pub min_connections: usize,
    /// 连接预热开关
    pub enable_warmup: bool,
    /// 自定义扩展因子序列
    pub expansion_factors: Vec<f64>,
}

impl Default for SmartPoolConfig {
    fn default() -> Self {
        Self {
            initial_size: 100,
            max_size: 2000,
            expansion_threshold: 0.8,   // 80%使用率触发扩展
            shrink_threshold: 0.3,      // 30%使用率触发收缩
            expansion_cooldown: Duration::from_secs(30),
            shrink_cooldown: Duration::from_secs(60),
            min_connections: 10,
            enable_warmup: true,
            expansion_factors: vec![2.0, 1.5, 1.2, 1.1], // Progressive expansion factors
        }
    }
}

impl SmartPoolConfig {
    /// 创建高性能配置预设
    pub fn high_performance() -> Self {
        Self {
            initial_size: 200,
            max_size: 5000,
            expansion_threshold: 0.75,
            shrink_threshold: 0.25,
            expansion_cooldown: Duration::from_secs(15),
            shrink_cooldown: Duration::from_secs(45),
            min_connections: 50,
            enable_warmup: true,
            expansion_factors: vec![2.5, 2.0, 1.5, 1.2, 1.1],
        }
    }
    
    /// 创建保守配置预设
    pub fn conservative() -> Self {
        Self {
            initial_size: 50,
            max_size: 1000,
            expansion_threshold: 0.9,
            shrink_threshold: 0.2,
            expansion_cooldown: Duration::from_secs(60),
            shrink_cooldown: Duration::from_secs(120),
            min_connections: 5,
            enable_warmup: false,
            expansion_factors: vec![1.5, 1.3, 1.2, 1.1],
        }
    }
    
    /// 验证配置有效性
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.initial_size == 0 {
            return Err(TransportError::config_error("smart_pool", "initial_size must be > 0"));
        }
        
        if self.max_size < self.initial_size {
            return Err(TransportError::config_error("smart_pool", "max_size must be >= initial_size"));
        }
        
        if self.expansion_threshold <= 0.0 || self.expansion_threshold >= 1.0 {
            return Err(TransportError::config_error("smart_pool", "expansion_threshold must be in (0.0, 1.0)"));
        }
        
        if self.shrink_threshold <= 0.0 || self.shrink_threshold >= 1.0 {
            return Err(TransportError::config_error("smart_pool", "shrink_threshold must be in (0.0, 1.0)"));
        }
        
        if self.expansion_threshold <= self.shrink_threshold {
            return Err(TransportError::config_error("smart_pool", "expansion_threshold must be > shrink_threshold"));
        }
        
        if self.expansion_factors.is_empty() {
            return Err(TransportError::config_error("smart_pool", "expansion_factors cannot be empty"));
        }
        
        for factor in &self.expansion_factors {
            if *factor <= 1.0 {
                return Err(TransportError::config_error("smart_pool", "all expansion_factors must be > 1.0"));
            }
        }
        
        Ok(())
    }
    
    /// 转换为ExpansionStrategy
    pub fn to_expansion_strategy(&self) -> ExpansionStrategy {
        ExpansionStrategy {
            factors: self.expansion_factors.clone(),
            current_factor_index: 0,
            expansion_threshold: self.expansion_threshold,
            shrink_threshold: self.shrink_threshold,
        }
    }
}

/// 性能监控专家配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// 启用详细监控
    pub enable_detailed_monitoring: bool,
    /// 监控数据保留时间
    pub monitoring_retention: Duration,
    /// 性能指标采样间隔
    pub sampling_interval: Duration,
    /// 启用性能分析
    pub enable_profiling: bool,
    /// 指标历史保留条数
    pub metrics_history_size: usize,
    /// 自动报告间隔
    pub auto_report_interval: Option<Duration>,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_detailed_monitoring: true,
            monitoring_retention: Duration::from_secs(3600), // 1小时
            sampling_interval: Duration::from_millis(100),
            enable_profiling: cfg!(debug_assertions),
            metrics_history_size: 1000,
            auto_report_interval: Some(Duration::from_secs(300)), // 5分钟
        }
    }
}

impl PerformanceConfig {
    /// 创建生产环境配置预设
    pub fn production() -> Self {
        Self {
            enable_detailed_monitoring: true,
            monitoring_retention: Duration::from_secs(7200), // 2小时
            sampling_interval: Duration::from_millis(200),
            enable_profiling: false,
            metrics_history_size: 2000,
            auto_report_interval: Some(Duration::from_secs(600)), // 10分钟
        }
    }
    
    /// 创建开发环境配置预设
    pub fn development() -> Self {
        Self {
            enable_detailed_monitoring: true,
            monitoring_retention: Duration::from_secs(1800), // 30分钟
            sampling_interval: Duration::from_millis(50),
            enable_profiling: true,
            metrics_history_size: 500,
            auto_report_interval: Some(Duration::from_secs(60)), // 1分钟
        }
    }
    
    /// 验证配置有效性
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.sampling_interval.as_millis() < 10 {
            return Err(TransportError::config_error("performance", "sampling_interval must be >= 10ms"));
        }
        
        if self.monitoring_retention.as_secs() < 60 {
            return Err(TransportError::config_error("performance", "monitoring_retention must be >= 60 seconds"));
        }
        
        if self.metrics_history_size == 0 {
            return Err(TransportError::config_error("performance", "metrics_history_size must be > 0"));
        }
        
        Ok(())
    }
}

/// 专家配置集合
#[derive(Debug, Clone)]
pub struct ExpertConfig {
    /// 智能连接池配置
    pub smart_pool: Option<SmartPoolConfig>,
    /// 性能监控配置
    pub performance: Option<PerformanceConfig>,
}

impl Default for ExpertConfig {
    fn default() -> Self {
        Self {
            smart_pool: None,
            performance: None,
        }
    }
}

impl ExpertConfig {
    /// 创建新的专家配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 验证所有配置
    pub fn validate(&self) -> Result<(), TransportError> {
        if let Some(ref config) = self.smart_pool {
            config.validate()?;
        }
        
        if let Some(ref config) = self.performance {
            config.validate()?;
        }
        
        Ok(())
    }
    
    /// 是否启用了任何专家配置
    pub fn has_expert_config(&self) -> bool {
        self.smart_pool.is_some() || self.performance.is_some()
    }
}
