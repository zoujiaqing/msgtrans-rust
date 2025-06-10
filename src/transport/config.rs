#[derive(Debug, Clone)]
pub struct TransportConfig { 
    pub global: GlobalConfig 
} 

impl Default for TransportConfig { 
    fn default() -> Self { 
        Self { 
            global: GlobalConfig::default() 
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
