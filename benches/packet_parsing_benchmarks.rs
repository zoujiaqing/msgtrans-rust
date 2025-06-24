/// 数据包解析性能基准测试
/// 
/// 对比：
/// 1. 传统方法 vs 零拷贝解析
/// 2. 单个数据包 vs 批量解析
/// 3. 不同大小数据包的解析性能
/// 4. 内存分配开销对比

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use msgtrans::packet::Packet;
use bytes::{BytesMut, Buf};
use std::io::{Cursor, Read};

/// 模拟传统的分段读取方法
struct TraditionalParser;

impl TraditionalParser {
    fn parse_packet_traditional(data: &[u8]) -> Result<Packet, Box<dyn std::error::Error>> {
        // 模拟传统方法：多次内存分配和拷贝
        
        // 步骤1：读取并复制头部
        if data.len() < 16 {
            return Err("数据不足".into());
        }
        let header = data[0..16].to_vec(); // 第一次拷贝
        
        // 步骤2：解析头部长度
        let payload_len = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let ext_header_len = u16::from_be_bytes([header[12], header[13]]) as usize;
        
        let total_len = 16 + ext_header_len + payload_len;
        if data.len() < total_len {
            return Err("数据包不完整".into());
        }
        
        // 步骤3：分别复制扩展头和负载
        let ext_header = if ext_header_len > 0 {
            data[16..16 + ext_header_len].to_vec() // 第二次拷贝
        } else {
            Vec::new()
        };
        
        let payload = if payload_len > 0 {
            data[16 + ext_header_len..total_len].to_vec() // 第三次拷贝
        } else {
            Vec::new()
        };
        
        // 步骤4：重构完整数据包
        let mut reconstructed = Vec::with_capacity(total_len);
        reconstructed.extend_from_slice(&header); // 第四次拷贝
        reconstructed.extend_from_slice(&ext_header); // 第五次拷贝
        reconstructed.extend_from_slice(&payload); // 第六次拷贝
        
        // 步骤5：解析
        Packet::from_bytes(&reconstructed).map_err(Into::into)
    }
}

/// 模拟优化的零拷贝解析方法
struct ZeroCopyParser {
    buffer: BytesMut,
}

impl ZeroCopyParser {
    fn new() -> Self {
        Self {
            buffer: BytesMut::new(),
        }
    }
    
    fn parse_packet_zerocopy(&mut self, data: &[u8]) -> Result<Option<Packet>, Box<dyn std::error::Error>> {
        // 模拟从网络读取到缓冲区
        self.buffer.extend_from_slice(data);
        
        // 检查是否有完整数据包
        if self.buffer.len() < 16 {
            return Ok(None);
        }

        // 零拷贝解析头部
        let header_bytes = &self.buffer[0..16];
        let payload_len = u32::from_be_bytes([header_bytes[4], header_bytes[5], header_bytes[6], header_bytes[7]]) as usize;
        let ext_header_len = u16::from_be_bytes([header_bytes[12], header_bytes[13]]) as usize;

        let total_packet_len = 16 + ext_header_len + payload_len;

        if self.buffer.len() < total_packet_len {
            return Ok(None);
        }

        // 零拷贝分割
        let packet_bytes = self.buffer.split_to(total_packet_len).freeze();
        
        // 解析
        let packet = Packet::from_bytes(&packet_bytes)?;
        Ok(Some(packet))
    }
    
    fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// 生成测试数据包
fn generate_test_packets(count: usize, payload_size: usize) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    
    for i in 0..count {
        let payload = vec![0u8; payload_size];
        let packet = Packet::one_way(i as u32, payload);
        packets.push(packet.to_bytes());
    }
    
    packets
}

/// 基准测试：传统解析 vs 零拷贝解析
fn bench_parsing_methods(c: &mut Criterion) {
    let mut group = c.benchmark_group("parsing_methods");
    
    // 测试不同大小的数据包
    let sizes = [64, 256, 1024, 4096, 16384];
    
    for size in sizes {
        let test_data = generate_test_packets(1, size);
        let packet_bytes = &test_data[0];
        
        // 传统方法
        group.bench_with_input(
            BenchmarkId::new("traditional", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let result = TraditionalParser::parse_packet_traditional(black_box(packet_bytes));
                    black_box(result)
                })
            },
        );
        
        // 零拷贝方法
        group.bench_with_input(
            BenchmarkId::new("zerocopy", size),
            &size,
            |b, _| {
                let mut parser = ZeroCopyParser::new();
                b.iter(|| {
                    parser.clear();
                    let result = parser.parse_packet_zerocopy(black_box(packet_bytes));
                    black_box(result)
                })
            },
        );
    }
    
    group.finish();
}

/// 基准测试：批量解析性能
fn bench_batch_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_parsing");
    group.throughput(Throughput::Elements(100));
    
    // 生成100个小数据包的连续字节流
    let packets = generate_test_packets(100, 64);
    let mut combined_data = Vec::new();
    for packet_data in &packets {
        combined_data.extend_from_slice(packet_data);
    }
    
    // 传统方法：逐个解析
    group.bench_function("traditional_individual", |b| {
        b.iter(|| {
            let mut results = Vec::new();
            for packet_data in &packets {
                let result = TraditionalParser::parse_packet_traditional(black_box(packet_data));
                results.push(result);
            }
            black_box(results)
        })
    });
    
    // 零拷贝方法：批量解析
    group.bench_function("zerocopy_batch", |b| {
        b.iter(|| {
            let mut parser = ZeroCopyParser::new();
            let mut results = Vec::new();
            
            // 模拟从流中读取所有数据
            parser.buffer.extend_from_slice(black_box(&combined_data));
            
            // 批量解析所有数据包
            loop {
                                 match parser.parse_packet_zerocopy(&[]) {
                     Ok(Some(packet)) => results.push(Ok(packet)),
                     Ok(None) => break,
                     Err(e) => {
                         results.push(Err(e));
                         break;
                     }
                 }
            }
            
            black_box(results)
        })
    });
    
    group.finish();
}

/// 基准测试：内存分配开销对比
fn bench_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");
    group.throughput(Throughput::Elements(1000));
    
    let packets = generate_test_packets(1000, 256);
    
    // 传统方法：每次都分配新内存
    group.bench_function("traditional_new_allocation", |b| {
        b.iter(|| {
            let mut results = Vec::new();
            for packet_data in &packets {
                let result = TraditionalParser::parse_packet_traditional(black_box(packet_data));
                results.push(result);
            }
            black_box(results)
        })
    });
    
    // 零拷贝方法：复用缓冲区
    group.bench_function("zerocopy_buffer_reuse", |b| {
        b.iter(|| {
            let mut parser = ZeroCopyParser::new();
            let mut results = Vec::new();
            
            for packet_data in &packets {
                parser.clear();
                match parser.parse_packet_zerocopy(black_box(packet_data)) {
                    Ok(Some(packet)) => results.push(Ok(packet)),
                    Ok(None) => {} // 需要更多数据
                    Err(e) => results.push(Err(e)),
                }
            }
            
            black_box(results)
        })
    });
    
    group.finish();
}

/// 基准测试：不同缓冲区大小的影响
fn bench_buffer_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_sizes");
    
    let buffer_sizes = [1024, 4096, 8192, 16384, 32768];
    let test_packets = generate_test_packets(50, 512);
    
    for buffer_size in buffer_sizes {
        group.bench_with_input(
            BenchmarkId::new("buffer_size", buffer_size),
            &buffer_size,
            |b, &size| {
                b.iter(|| {
                    let mut parser = ZeroCopyParser::new();
                    parser.buffer.reserve(size);
                    
                    let mut results = Vec::new();
                    
                    for packet_data in &test_packets {
                        parser.clear();
                        match parser.parse_packet_zerocopy(black_box(packet_data)) {
                            Ok(Some(packet)) => results.push(packet),
                            Ok(None) => {} 
                            Err(_) => break,
                        }
                    }
                    
                    black_box(results)
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_parsing_methods,
    bench_batch_parsing,
    bench_memory_allocation,
    bench_buffer_sizes
);
criterion_main!(benches); 