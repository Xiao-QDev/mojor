use std::fmt::Write as _;

use bytes::{BufMut, BytesMut};

use crate::error::{Error, Result};

pub struct BinaryWriter {
    buf: BytesMut,
}

impl BinaryWriter {
    pub fn new() -> Self {
        Self::with_capacity(64)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: BytesMut::with_capacity(capacity),
        }
    }

    pub fn put_u8(&mut self, v: u8) {
        self.buf.put_u8(v);
    }

    pub fn put_u16(&mut self, v: u16) {
        self.buf.put_u16(v);
    }

    pub fn put_u32(&mut self, v: u32) {
        self.buf.put_u32(v);
    }

    pub fn put_i32(&mut self, v: i32) {
        self.buf.put_i32(v);
    }

    pub fn put_f32(&mut self, v: f32) {
        self.buf.put_f32(v);
    }

    pub fn put_i64(&mut self, v: i64) {
        self.buf.put_i64(v);
    }

    // 小端 供 NBT 与部分协议字段使用
    pub fn put_lint(&mut self, v: i32) {
        self.buf.put_i32(v.to_le());
    }

    pub fn put_lu16(&mut self, v: u16) {
        self.buf.put_u16(v.to_le());
    }

    pub fn put_llong(&mut self, v: i64) {
        self.buf.put_i64(v.to_le());
    }

    pub fn put_u64(&mut self, v: u64) {
        self.buf.put_u64(v);
    }

    // 字符串为 u16 大端长度 + UTF-8 字节
    pub fn put_string(&mut self, s: &str) -> Result<()> {
        let b = s.as_bytes();
        let len = u16::try_from(b.len()).map_err(|_| Error::StringTooLong(b.len()))?;
        self.buf.put_u16(len);
        self.buf.put_slice(b);
        Ok(())
    }

    // RakNet 序号等字段使用 3 字节大端
    pub fn put_u24(&mut self, v: u32) {
        self.buf.put_u8((v >> 16) as u8);
        self.buf.put_u8((v >> 8) as u8);
        self.buf.put_u8(v as u8);
    }

    // RakNet 部分字段使用 3 字节小端
    pub fn put_lu24(&mut self, v: u32) {
        self.buf.put_u8(v as u8);
        self.buf.put_u8((v >> 8) as u8);
        self.buf.put_u8((v >> 16) as u8);
    }

    pub fn put_bytes(&mut self, b: &[u8]) {
        self.buf.put_slice(b);
    }

    pub fn freeze(self) -> BytesMut {
        self.buf
    }
}

impl Default for BinaryWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BinaryReader<'a> {
    buf: &'a [u8],
    offset: usize,
}

impl<'a> BinaryReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, offset: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.offset)
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let left = self.remaining();
        if left < n {
            return Err(Error::TooShort {
                need: n,
                actual: left,
            });
        }
        let slice = &self.buf[self.offset..self.offset + n];
        self.offset += n;
        Ok(slice)
    }

    pub fn get_u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn get_u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn get_u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn get_i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    // 小端 u16 元数据与 NBT 长度用
    pub fn get_lu16(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn get_f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.get_u32()?))
    }

    pub fn get_i64(&mut self) -> Result<i64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(i64::from_be_bytes(a))
    }

    pub fn get_lint(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn get_llong(&mut self) -> Result<i64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(i64::from_le_bytes(a))
    }

    pub fn get_uuid(&mut self) -> Result<[u8; 16]> {
        let b = self.take(16)?;
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(b);
        Ok(uuid)
    }

    pub fn get_u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }

    pub fn get_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.take(n)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n)?;
        Ok(())
    }

    pub fn get_u24(&mut self) -> Result<u32> {
        let b = self.take(3)?;
        Ok(u32::from_be_bytes([0, b[0], b[1], b[2]]))
    }

    pub fn get_lu24(&mut self) -> Result<u32> {
        let b = self.take(3)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], 0]))
    }

    pub fn get_string(&mut self) -> Result<String> {
        let len = self.get_u16()? as usize;
        let b = self.take(len)?;
        String::from_utf8(b.to_vec()).map_err(|_| Error::InvalidUtf8)
    }

    pub fn check_magic(&mut self, expected: &[u8; 16]) -> Result<()> {
        let got = self.take(16)?;
        if got != &expected[..] {
            return Err(Error::BadMagic);
        }
        Ok(())
    }
}

pub fn hexdump(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i % 16 == 0 {
            if i > 0 {
                out.push('\n');
            }
            let _ = write!(out, "{i:04x}: ");
        }
        let _ = write!(out, "{b:02x} ");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triad_roundtrip() {
        let mut w = BinaryWriter::new();
        w.put_u24(0x12_34_56);
        w.put_lu24(0x12_34_56);
        let buf = w.freeze();
        assert_eq!(&buf[..3], &[0x12, 0x34, 0x56]);
        assert_eq!(&buf[3..6], &[0x56, 0x34, 0x12]);
        let mut r = BinaryReader::new(&buf);
        assert_eq!(r.get_u24().unwrap(), 0x12_34_56);
        assert_eq!(r.get_lu24().unwrap(), 0x12_34_56);
    }

    #[test]
    fn string_roundtrip_and_length_limit() {
        let mut w = BinaryWriter::new();
        w.put_string("mojor").unwrap();
        let buf = w.freeze();
        let mut r = BinaryReader::new(&buf);
        assert_eq!(r.get_string().unwrap(), "mojor");

        let mut w = BinaryWriter::new();
        assert!(matches!(
            w.put_string(&"x".repeat(65536)),
            Err(Error::StringTooLong(65536))
        ));
    }

    #[test]
    fn rejects_invalid_utf8() {
        let mut w = BinaryWriter::new();
        w.put_u16(1);
        w.put_u8(0xff);
        let buf = w.freeze();
        let mut r = BinaryReader::new(&buf);
        assert!(matches!(r.get_string(), Err(Error::InvalidUtf8)));
    }
}
