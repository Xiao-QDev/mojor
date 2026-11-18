// 通用实体包 0x98 原版 getId()=10 线上 10-114=0x98
// 线序(与 MCPE 0.14.1 原版 AddEntityPacket::write / PHP 核心三方一致):
//   pid(u8) | eid(i64 BE) | type(i32 BE) | x y z(f32 BE) | vx vy vz(f32 BE)
//   | yaw(f32 BE) | pitch(f32 BE) | metadata(blob, 内数值小端, 0x7f 结尾) | linkCount(u16 BE)
// 下落方块实体 type=66 metadata 只需 DATA_VARIANT(0x14) = block_id | data<<8
use bytes::Bytes;

use crate::network::protocol::entity_data::Metadata;
use crate::network::protocol::pid::ADD_ENTITY;
use crate::utils::binary::BinaryWriter;

// MCPE 0.14 实体类型 与 EntityFactory.cpp / PHP FallingSand::NETWORK_ID 一致
pub const ENTITY_TYPE_FALLING_BLOCK: i32 = 66;

pub struct AddEntityPacket<'a> {
    pub eid: i64,
    pub entity_type: i32,
    pub position: (f32, f32, f32),
    pub velocity: (f32, f32, f32),
    // 元数据 blob 已经编码好(含 0x7f 结尾)
    pub metadata: &'a [u8],
}

impl AddEntityPacket<'_> {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(64);
        w.put_u8(ADD_ENTITY);
        w.put_i64(self.eid);
        w.put_i32(self.entity_type);
        w.put_f32(self.position.0);
        w.put_f32(self.position.1);
        w.put_f32(self.position.2);
        w.put_f32(self.velocity.0);
        w.put_f32(self.velocity.1);
        w.put_f32(self.velocity.2);
        // 原版两个旋转 f32 下落方块都是 0 无差别
        w.put_f32(0.0);
        w.put_f32(0.0);
        // metadata blob 原样写入(PHP putBlob = u32 长度 + 数据?)
        // 注意: 此处的 AddEntity 元数据是裸字节流直到 0x7f, 不带长度前缀
        // (SynchedEntityData::pack 直接把条目写进 BitStream, 没有 blob 长度)
        w.put_bytes(self.metadata);
        // linkCount = 0
        w.put_u16(0);
        Bytes::from(w.freeze())
    }
}

// 下落方块实体的最小元数据集合 只带 DATA_VARIANT
pub fn falling_block_metadata(block_id: u8, data: u8) -> Vec<u8> {
    let mut m = Metadata::new();
    let variant = block_id as i32 | ((data as i32) << 8);
    m.put_int(crate::network::protocol::entity_data::DATA_VARIANT, variant);
    m.encode()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_entity_layout() {
        let meta = falling_block_metadata(12, 1);
        // 0x54 = (TYPE_INT=2 << 5) | 0x14, 值 = 12 | 1<<8 = 268 LE, 0x7f
        assert_eq!(meta, vec![0x54, 0x0c, 0x01, 0x00, 0x00, 0x7f]);

        let data = AddEntityPacket {
            eid: 42,
            entity_type: ENTITY_TYPE_FALLING_BLOCK,
            position: (0.5, 10.5, 0.5),
            velocity: (0.0, -0.04, 0.0),
            metadata: &meta,
        }
        .encode();
        assert_eq!(data[0], ADD_ENTITY);
        assert_eq!(&data[1..9], &42i64.to_be_bytes());
        assert_eq!(&data[9..13], &66i32.to_be_bytes());
        assert_eq!(&data[13..17], &0.5f32.to_be_bytes());
        assert_eq!(&data[17..21], &10.5f32.to_be_bytes());
        // pid1 + eid8 + type4 + pos12 + vel12 + yaw/pitch8 = 45, 其后是 metadata
        assert_eq!(&data[45..51], &meta);
        // 末尾 linkCount=0
        assert_eq!(&data[data.len() - 2..], &0u16.to_be_bytes());
        assert_eq!(data.len(), 53);
    }
}
