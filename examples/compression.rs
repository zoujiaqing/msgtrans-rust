use msgtrans::compression::CompressionMethod;

fn main() {
    let data = b"Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!Hello, this is a test message to be compressed!";

    let method = CompressionMethod::Zstd;

    match method.encode(data) {
        Ok(compressed) => {
            println!("Compressed size: {}", compressed.len());

            match method.decode(&compressed) {
                Ok(decompressed) => {
                    println!("Decompressed size: {}", decompressed.len());
                    // println!("Decompressed data: {:?}", String::from_utf8_lossy(&decompressed));
                }
                Err(e) => {
                    eprintln!("Failed to decompress data: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to compress data: {}", e);
        }
    }
}