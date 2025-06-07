use msgtrans::protocol::{QuicConfig, ProtocolConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .init();

    println!("🚀 QUIC API 演示");
    println!("================");

    // 1. 默认模式 - 自动生成自签名证书（对用户透明）
    println!("📋 1. 默认模式（自动生成自签名证书）");
    let default_config = QuicConfig::new("127.0.0.1:0")?
        .with_max_idle_timeout(Duration::from_secs(30))
        .with_max_concurrent_streams(100);
    
    println!("   ✅ 配置创建成功 - 将自动生成自签名证书");
    println!("   📝 证书PEM: {:?}", default_config.cert_pem);
    println!("   📝 密钥PEM: {:?}", default_config.key_pem);
    
    // 验证配置
    default_config.validate()?;
    println!("   ✅ 配置验证通过");

    // 2. 安全模式 - 提供PEM证书内容
    println!("\n📋 2. 安全模式（提供PEM证书内容）");
    
    // 模拟从数据库或环境变量获取的证书内容
    let cert_pem = r#"-----BEGIN CERTIFICATE-----
MIIBhTCCASugAwIBAgIJANjFD7Q+j1nKMA0GCSqGSIb3DQEBCwUAMBQxEjAQBgNV
BAMMCWxvY2FsaG9zdDAeFw0yNDAxMDEwMDAwMDBaFw0yNTAxMDEwMDAwMDBaMBQx
EjAQBgNVBAMMCWxvY2FsaG9zdDBcMA0GCSqGSIb3DQEBAQUAA0sAMEgCQQDhRp2w
// 这是一个示例证书（实际应用中请使用真实证书）
-----END CERTIFICATE-----"#;

    let key_pem = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC6K+9ZqQ2HKnN5
// 这是一个示例私钥（实际应用中请使用真实私钥）
-----END PRIVATE KEY-----"#;

    // 注意：这只是演示API，实际运行时会因为示例证书格式而失败
    // 在真实应用中，您会从数据库、配置文件或环境变量中获取真实的PEM内容
    println!("   📜 从数据库/配置中获取证书内容...");
    
    let secure_config = QuicConfig::new("127.0.0.1:0")?
        .with_tls_cert(cert_pem, key_pem)
        .with_max_idle_timeout(Duration::from_secs(60))
        .with_max_concurrent_streams(200);

    println!("   ✅ 安全配置创建成功");
    println!("   📝 使用提供的证书: {} 字符", cert_pem.len());
    println!("   📝 使用提供的密钥: {} 字符", key_pem.len());
    
    // 验证配置（注意：这里可能会因为示例证书而在实际使用时失败）
    secure_config.validate()?;
    println!("   ✅ 安全配置验证通过");

    // 3. 展示配置构建器模式
    println!("\n📋 3. 配置构建器模式");
    
    let builder_config = QuicConfig::new("0.0.0.0:8003")?
        .with_max_concurrent_streams(500)
        .with_max_idle_timeout(Duration::from_secs(120))
        .with_keep_alive_interval(Some(Duration::from_secs(30)))
        .with_initial_rtt(Duration::from_millis(100));
    
    println!("   ✅ 使用构建器模式创建配置");
    println!("   📊 最大并发流: {}", builder_config.max_concurrent_streams);
    println!("   ⏱️  最大空闲时间: {:?}", builder_config.max_idle_timeout);
    println!("   💓 心跳间隔: {:?}", builder_config.keep_alive_interval);

    println!("\n🎯 API设计优势:");
    println!("   ✨ 自签名证书完全透明 - 用户无需关心证书生成细节");
    println!("   🔐 直接接受PEM内容 - 支持从数据库、环境变量等多种来源");
    println!("   🛡️  配置验证 - 在创建时就检查配置的正确性");
    println!("   🔧 构建器模式 - 流畅的API体验");
    println!("   📦 类型安全 - 编译时配置验证");

    println!("\n✅ QUIC API 演示完成!");
    
    Ok(())
} 