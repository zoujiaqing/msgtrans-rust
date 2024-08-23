use flate2::{write::ZlibEncoder, read::ZlibDecoder, Compression};
use zstd::stream::{Encoder as ZstdEncoder, Decoder as ZstdDecoder};
use std::io::{Read, Write, Result as IoResult};

pub enum CompressionMethod {
    None = 0,
    Zstd = 1,
    Zlib = 2,
}

impl CompressionMethod {
    pub fn encode(&self, data: &[u8]) -> IoResult<Vec<u8>> {
        match self {
            CompressionMethod::None => Ok(data.to_vec()),
            CompressionMethod::Zstd => {
                let mut encoder = ZstdEncoder::new(Vec::new(), 0)?;
                encoder.write_all(data)?;
                encoder.finish()
            }
            CompressionMethod::Zlib => {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(data)?;
                encoder.finish()
            }
        }
    }

    pub fn decode(&self, data: &[u8]) -> IoResult<Vec<u8>> {
        match self {
            CompressionMethod::None => Ok(data.to_vec()),
            CompressionMethod::Zstd => {
                let mut decoder = ZstdDecoder::new(data)?;
                let mut decoded_data = Vec::new();
                decoder.read_to_end(&mut decoded_data)?;
                Ok(decoded_data)
            }
            CompressionMethod::Zlib => {
                let mut decoder = ZlibDecoder::new(data);
                let mut decoded_data = Vec::new();
                decoder.read_to_end(&mut decoded_data)?;
                Ok(decoded_data)
            }
        }
    }
}