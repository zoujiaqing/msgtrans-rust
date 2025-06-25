/// 数据包封装和解包验证示例
/// 
/// 展示msgtrans统一数据包系统的基本序列化和反序列化功能

use msgtrans::packet::{Packet, PacketType};

fn main() {
    println!("🚀 数据包序列化与反序列化测试");
    
    // 测试消息
    let test_message = "Hello, Packet World! 你好，数据包世界！";
    let extend_data = "Extended payload for testing serialization and deserialization";
    
    // 创建不同类型的数据包
    let packets = vec![
        ("单向消息", Packet::one_way(101, test_message)),
        ("请求消息", Packet::request(102, extend_data)),
        ("响应消息", Packet::response(103, "Response test")),
        ("二进制数据", Packet::one_way(105, &[0x00u8, 0x01, 0x02, 0x03, 0xFF, 0xFE][..])),
    ];
    
    // 测试每个数据包
    for (name, packet) in packets {
        println!("  {} - 类型: {:?}, ID: {}, 负载: {} bytes", 
            name, 
            packet.header.packet_type, 
            packet.header.message_id, 
            packet.payload.len()
        );
        
        // 测试序列化
        let serialized = packet.to_bytes();
        println!("    序列化: {} bytes", serialized.len());
        
        // 测试反序列化
        match Packet::from_bytes(&serialized) {
            Ok(recovered) => {
                println!("    反序列化成功: {} bytes", recovered.payload.len());
                
                // 验证数据一致性
                if packet == recovered {
                    println!("    ✅ 数据一致性检查通过");
                } else {
                    println!("    ❌ 数据一致性检查失败");
                }
            }
            Err(e) => {
                println!("    ❌ 反序列化失败: {:?}", e);
            }
        }
        println!();
    }
    
    // 详细的序列化测试
    println!("📋 详细序列化测试");
    
    let test_packet = Packet::one_way(999, test_message);
    
    println!("  原始数据包:");
    println!("    类型: {:?}", test_packet.header.packet_type);
    println!("    消息ID: {}", test_packet.header.message_id);
    println!("    负载长度: {} bytes", test_packet.payload.len());
    if let Some(text) = test_packet.payload_as_string() {
        println!("    负载内容: \"{}\"", text);
    }
    
    // 序列化
    let bytes = test_packet.to_bytes();
    println!("  序列化后: {} bytes", bytes.len());
    println!("    前16字节（头部）: {:02X?}", &bytes[0..16.min(bytes.len())]);
    
    // 反序列化
    match Packet::from_bytes(&bytes) {
        Ok(recovered_packet) => {
            println!("  反序列化:");
            println!("    类型: {:?}", recovered_packet.header.packet_type);
            println!("    消息ID: {}", recovered_packet.header.message_id);
            println!("    负载长度: {} bytes", recovered_packet.payload.len());
            if let Some(text) = recovered_packet.payload_as_string() {
                println!("    负载内容: \"{}\"", text);
            }
            
            // 完整性检查
            if test_packet == recovered_packet {
                println!("  ✅ 完整性检查通过");
            } else {
                println!("  ❌ 完整性检查失败");
            }
        }
        Err(e) => {
            println!("  ❌ 反序列化失败: {:?}", e);
        }
    }
    
    println!("🎯 数据包测试完成");
} 