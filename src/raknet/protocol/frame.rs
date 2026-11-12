use bytes::Bytes;

use crate::error::{Error, Result};
use crate::utils::binary::{BinaryReader, BinaryWriter};

pub const ID_DATA_PACKET_BASE: u8 = 0x80;
pub const ID_DATA_PACKET_4: u8 = 0x84;
pub const ID_NACK: u8 = 0xa0;
pub const ID_ACK: u8 = 0xc0;

pub const RELIABILITY_UNRELIABLE: u8 = 0;
pub const RELIABILITY_RELIABLE: u8 = 2;
pub const RELIABILITY_RELIABLE_ORDERED: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitInfo {
    pub count: u32,
    pub id: u16,
    pub index: u32,
}

#[derive(Debug, Clone)]
pub struct EncapsulatedPacket {
    pub reliability: u8,
    pub message_index: Option<u32>,
    pub order_index: Option<u32>,
    pub order_channel: u8,
    pub split: Option<SplitInfo>,
    pub payload: Bytes,
}

impl EncapsulatedPacket {
    pub fn unreliable(payload: impl Into<Bytes>) -> Self {
        Self {
            reliability: RELIABILITY_UNRELIABLE,
            message_index: None,
            order_index: None,
            order_channel: 0,
            split: None,
            payload: payload.into(),
        }
    }

    pub fn reliable(payload: impl Into<Bytes>, message_index: u32) -> Self {
        Self {
            reliability: RELIABILITY_RELIABLE,
            message_index: Some(message_index),
            order_index: None,
            order_channel: 0,
            split: None,
            payload: payload.into(),
        }
    }

    pub fn reliable_ordered(
        payload: impl Into<Bytes>,
        message_index: u32,
        order_index: u32,
        order_channel: u8,
    ) -> Self {
        Self {
            reliability: RELIABILITY_RELIABLE_ORDERED,
            message_index: Some(message_index),
            order_index: Some(order_index),
            order_channel,
            split: None,
            payload: payload.into(),
        }
    }

    pub fn encoded_len(&self) -> usize {
        3 + self.payload.len()
            + if self.message_index.is_some() { 3 } else { 0 }
            + if self.order_index.is_some() { 4 } else { 0 }
            + if self.split.is_some() { 10 } else { 0 }
    }

    pub fn encode(&self, w: &mut BinaryWriter) {
        let mut flags = self.reliability << 5;
        if self.split.is_some() {
            flags |= 0x10;
        }
        w.put_u8(flags);
        // 长度单位是 bit
        let bit_len = u16::try_from(self.payload.len().saturating_mul(8)).unwrap_or(u16::MAX);
        w.put_u16(bit_len);

        if let Some(mi) = self.message_index {
            w.put_lu24(mi);
        }
        if let Some(oi) = self.order_index {
            w.put_lu24(oi);
            w.put_u8(self.order_channel);
        }
        if let Some(s) = &self.split {
            w.put_u32(s.count);
            w.put_u16(s.id);
            w.put_u32(s.index);
        }
        w.put_bytes(&self.payload);
    }

    pub fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut r = BinaryReader::new(buf);
        let flags = r.get_u8()?;
        let reliability = flags >> 5;
        let has_split = flags & 0x10 != 0;
        let bit_len = r.get_u16()? as usize;
        let byte_len = bit_len.div_ceil(8);

        let message_index = if reliability >= 2 && reliability != 5 {
            Some(r.get_lu24()?)
        } else {
            None
        };
        let (order_index, order_channel) =
            if reliability > 0 && reliability <= 4 && reliability != 2 {
                (Some(r.get_lu24()?), r.get_u8()?)
            } else {
                (None, 0)
            };
        let split = if has_split {
            Some(SplitInfo {
                count: r.get_u32()?,
                id: r.get_u16()?,
                index: r.get_u32()?,
            })
        } else {
            None
        };
        let payload = Bytes::copy_from_slice(r.get_bytes(byte_len)?);

        Ok((
            Self {
                reliability,
                message_index,
                order_index,
                order_channel,
                split,
                payload,
            },
            r.offset(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct DataFrame {
    pub pid: u8,
    pub seq: u32,
    pub packets: Vec<EncapsulatedPacket>,
}

impl DataFrame {
    pub fn new(seq: u32, packets: Vec<EncapsulatedPacket>) -> Self {
        Self {
            pid: ID_DATA_PACKET_4,
            seq,
            packets,
        }
    }

    pub fn encoded_len(&self) -> usize {
        4 + self
            .packets
            .iter()
            .map(EncapsulatedPacket::encoded_len)
            .sum::<usize>()
    }

    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(self.encoded_len());
        w.put_u8(self.pid);
        w.put_lu24(self.seq);
        for pk in &self.packets {
            pk.encode(&mut w);
        }
        Bytes::from(w.freeze())
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if !(ID_DATA_PACKET_BASE..=0x8f).contains(&pid) {
            return Err(Error::BadPacketId(pid));
        }
        let seq = r.get_lu24()?;
        let mut packets = Vec::new();
        while r.remaining() > 0 {
            let (pk, used) = EncapsulatedPacket::decode(&buf[r.offset()..])?;
            if used == 0 {
                break;
            }
            r.skip(used)?;
            packets.push(pk);
        }
        Ok(Self { pid, seq, packets })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Ack {
    pub sequences: Vec<u32>,
}

enum Record {
    Single(u32),
    Range(u32, u32),
}

fn build_records(sequences: &[u32]) -> Vec<Record> {
    let mut records = Vec::new();
    let mut iter = sequences.iter().copied().peekable();
    while let Some(start) = iter.next() {
        let mut end = start;
        while let Some(&next) = iter.peek() {
            if next == end.saturating_add(1) {
                end = next;
                iter.next();
            } else {
                break;
            }
        }
        if start == end {
            records.push(Record::Single(start));
        } else {
            records.push(Record::Range(start, end));
        }
    }
    records
}

impl Ack {
    pub fn encode(&self, pid: u8) -> Bytes {
        let records = build_records(&self.sequences);
        let mut w = BinaryWriter::with_capacity(3 + records.len() * 7);
        w.put_u8(pid);
        w.put_u16(records.len() as u16);
        for record in records {
            match record {
                Record::Single(seq) => {
                    w.put_u8(0x01);
                    w.put_lu24(seq);
                }
                Record::Range(start, end) => {
                    w.put_u8(0x00);
                    w.put_lu24(start);
                    w.put_lu24(end);
                }
            }
        }
        Bytes::from(w.freeze())
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let _pid = r.get_u8()?;
        let count = r.get_u16()? as usize;
        let mut sequences = Vec::new();

        for _ in 0..count.min(4096) {
            if r.remaining() < 1 {
                break;
            }
            if r.get_u8()? == 0 {
                let start = r.get_lu24()?;
                let mut end = r.get_lu24()?;
                if end.saturating_sub(start) > 512 {
                    end = start.saturating_add(512);
                }
                for seq in start..=end {
                    sequences.push(seq);
                }
            } else {
                sequences.push(r.get_lu24()?);
            }
        }

        sequences.sort_unstable();
        sequences.dedup();
        Ok(Self { sequences })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encapsulated_reliable_ordered_roundtrip() {
        let pk =
            EncapsulatedPacket::reliable_ordered(Bytes::from_static(b"\x8e\x8fhello"), 7, 3, 2);
        let mut w = BinaryWriter::new();
        pk.encode(&mut w);
        let buf = w.freeze();

        assert_eq!(buf[0], (RELIABILITY_RELIABLE_ORDERED << 5));
        assert_eq!(u16::from_be_bytes([buf[1], buf[2]]) as usize, 7 * 8);

        let (decoded, used) = EncapsulatedPacket::decode(&buf).unwrap();
        assert_eq!(used, buf.len());
        assert_eq!(decoded.message_index, Some(7));
        assert_eq!(decoded.order_index, Some(3));
        assert_eq!(decoded.order_channel, 2);
        assert_eq!(&decoded.payload[..], b"\x8e\x8fhello");
    }

    #[test]
    fn encapsulated_split_roundtrip() {
        let mut pk = EncapsulatedPacket::reliable(Bytes::from_static(b"chunk"), 9);
        pk.split = Some(SplitInfo {
            count: 4,
            id: 65_535,
            index: 3,
        });
        let mut w = BinaryWriter::new();
        pk.encode(&mut w);
        let buf = w.freeze();

        let (decoded, used) = EncapsulatedPacket::decode(&buf).unwrap();
        assert_eq!(used, buf.len());
        assert_eq!(
            decoded.split,
            Some(SplitInfo {
                count: 4,
                id: 65_535,
                index: 3
            })
        );
        assert_eq!(&decoded.payload[..], b"chunk");
    }

    #[test]
    fn dataframe_roundtrip_with_two_packets() {
        let frame = DataFrame::new(
            0x12_34_56,
            vec![
                EncapsulatedPacket::unreliable(Bytes::from_static(b"\x09abc")),
                EncapsulatedPacket::reliable(Bytes::from_static(b"\x13def"), 1),
            ],
        );
        let data = frame.encode();
        let decoded = DataFrame::decode(&data).unwrap();
        assert_eq!(decoded.seq, 0x12_34_56);
        assert_eq!(decoded.packets.len(), 2);
        assert_eq!(&decoded.packets[0].payload[..], b"\x09abc");
        assert_eq!(&decoded.packets[1].payload[..], b"\x13def");
        assert_eq!(decoded.packets[1].message_index, Some(1));
    }

    #[test]
    fn ack_merges_contiguous_sequences() {
        let ack = Ack {
            sequences: vec![1, 2, 3, 5, 8, 9],
        };
        let data = ack.encode(ID_ACK);
        assert_eq!(data[0], ID_ACK);
        assert_eq!(u16::from_be_bytes([data[1], data[2]]), 3);
        // 1..3 range, 5 single, 8..9 range
        assert_eq!(data[3], 0x00);
        assert_eq!(data[4..7], [0x01, 0x00, 0x00]);
        assert_eq!(data[7..10], [0x03, 0x00, 0x00]);

        let decoded = Ack::decode(&data).unwrap();
        assert_eq!(decoded.sequences, vec![1, 2, 3, 5, 8, 9]);
    }

    #[test]
    fn nack_keeps_gaps() {
        let ack = Ack {
            sequences: vec![2, 4, 6],
        };
        let data = ack.encode(ID_NACK);
        let decoded = Ack::decode(&data).unwrap();
        assert_eq!(decoded.sequences, vec![2, 4, 6]);
    }
}
