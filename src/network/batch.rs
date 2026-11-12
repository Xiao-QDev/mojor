use std::io::{Read, Write};

use bytes::Bytes;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::error::{Error, Result};

// 解压上限 防止压缩炸弹
pub const MAX_BATCH_SIZE: usize = 64 * 1024 * 1024;

// buf 为 BatchPacket 去掉 0x8e 0x92 之后的内容 即 [i32 BE 长度][zlib 数据]
// 解压后内部结构为若干 [i32 BE 长度][0x8e][pid][body]
pub fn decode(buf: &[u8]) -> Result<Vec<Bytes>> {
    if buf.len() < 4 {
        return Err(Error::TooShort {
            need: 4,
            actual: buf.len(),
        });
    }
    let len = i32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if len < 0 {
        return Err(Error::Zlib);
    }
    let len = len as usize;
    if 4 + len > buf.len() {
        return Err(Error::TooShort {
            need: 4 + len,
            actual: buf.len(),
        });
    }
    let compressed = &buf[4..4 + len];

    let mut decoder = ZlibDecoder::new(compressed).take(MAX_BATCH_SIZE as u64);
    let mut data = Vec::new();
    decoder.read_to_end(&mut data).map_err(|_| Error::Zlib)?;

    let mut packets = Vec::new();
    let mut offset = 0usize;
    while offset + 4 <= data.len() {
        let len = i32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if len < 2 || offset + len > data.len() {
            break;
        }

        let packet = &data[offset..offset + len];
        offset += len;
        if packet[0] == 0x8e {
            packets.push(Bytes::copy_from_slice(packet));
        }
    }
    Ok(packets)
}

// 返回 0x92 + i32 长度 + zlib 数据 外层调用方需补 0x8e 前缀
pub fn encode(packets: &[Bytes]) -> Result<Bytes> {
    let mut raw = Vec::new();
    for packet in packets {
        let len = i32::try_from(packet.len()).map_err(|_| Error::BatchTooLarge)?;
        raw.extend_from_slice(&len.to_be_bytes());
        raw.extend_from_slice(packet);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw).map_err(|_| Error::Zlib)?;
    let compressed = encoder.finish().map_err(|_| Error::Zlib)?;

    let mut out = Vec::with_capacity(1 + 4 + compressed.len());
    out.push(0x92);
    let len = i32::try_from(compressed.len()).map_err(|_| Error::BatchTooLarge)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&compressed);
    Ok(Bytes::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_roundtrip() {
        let packets = vec![
            Bytes::from_static(b"\x8e\x90\x00\x00\x00\x00"),
            Bytes::from_static(b"\x8e\x93hello"),
        ];
        let encoded = encode(&packets).unwrap();
        assert_eq!(encoded[0], 0x92);

        let decoded = decode(&encoded[1..]).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(&decoded[0][..], &packets[0][..]);
        assert_eq!(&decoded[1][..], &packets[1][..]);
    }

    #[test]
    fn batch_skips_packets_without_prefix() {
        let raw = {
            let mut raw = Vec::new();
            raw.extend_from_slice(&(2i32).to_be_bytes());
            raw.extend_from_slice(b"ab");
            raw
        };
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut framed = Vec::new();
        framed.extend_from_slice(&(compressed.len() as i32).to_be_bytes());
        framed.extend_from_slice(&compressed);

        let decoded = decode(&framed).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn batch_rejects_truncated_frames() {
        let raw = {
            let mut raw = Vec::new();
            raw.extend_from_slice(&(10i32).to_be_bytes());
            raw.extend_from_slice(b"\x8e\x90");
            raw
        };
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut framed = Vec::new();
        framed.extend_from_slice(&(compressed.len() as i32).to_be_bytes());
        framed.extend_from_slice(&compressed);

        assert!(decode(&framed).unwrap().is_empty());
    }
}
