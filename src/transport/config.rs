#[derive(Debug, Clone)]
pub struct TransportConfig { 
    pub global: GlobalConfig,
    /// 优雅关闭超时时间
    pub graceful_timeout: std::time::Duration,
} 

impl Default for TransportConfig { 
    fn default() -> Self { 
        Self { 
            global: GlobalConfig::default(),
            graceful_timeout: std::time::Duration::from_secs(5),
        } 
    } 
}

impl TransportConfig {
    pub fn validate(&self) -> Result<(), crate::error::TransportError> {
        // TODO: 实现配置验证逻辑
        Ok(())
    }
}

#[derive(Default, Debug, Clone)] 
pub struct GlobalConfig;
