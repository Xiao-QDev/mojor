use bytes::Bytes;

use crate::network::protocol::pid::LEVEL_EVENT;
use crate::utils::binary::BinaryWriter;

// 方块被挖掉时的碎屑粒子 Nukkit LevelEventPacket.EVENT_PARTICLE_DESTROY
pub const EVENT_PARTICLE_DESTROY: i16 = 2001;

// 门开关音效 Nukkit LevelEventPacket.EVENT_SOUND_DOOR
pub const EVENT_SOUND_DOOR: i16 = 1003;

// 0xa2 | evid(i16) | x,y,z(f32) | data(i32)
// data 的低 8 位是方块 id 高 4 位是 meta 客户端靠它挑碎屑贴图
#[derive(Debug, Clone, Copy)]
pub struct LevelEventPacket {
    pub evid: i16,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub data: i32,
}

impl LevelEventPacket {
    // 方块破坏粒子 坐标取方块中心 与 Nukkit DestroyBlockParticle 一致
    pub fn particle_destroy(x: i32, y: i32, z: i32, block_id: u8, block_data: u8) -> Self {
        Self {
            evid: EVENT_PARTICLE_DESTROY,
            x: x as f32 + 0.5,
            y: y as f32 + 0.5,
            z: z as f32 + 0.5,
            data: block_id as i32 + ((block_data as i32) << 12),
        }
    }

    // 门开关音效 位置取方块中心 data 是 Nukkit GenericSound 的 pitch 默认 0
    pub fn sound_door(x: i32, y: i32, z: i32) -> Self {
        Self {
            evid: EVENT_SOUND_DOOR,
            x: x as f32 + 0.5,
            y: y as f32 + 0.5,
            z: z as f32 + 0.5,
            data: 0,
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(15);
        w.put_u8(LEVEL_EVENT);
        w.put_u16(self.evid as u16);
        w.put_f32(self.x);
        w.put_f32(self.y);
        w.put_f32(self.z);
        w.put_i32(self.data);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destroy_layout() {
        let data = LevelEventPacket::particle_destroy(16, -4, 3, 17, 1).encode();
        assert_eq!(data[0], LEVEL_EVENT);
        assert_eq!(&data[1..3], &2001i16.to_be_bytes());
        assert_eq!(&data[3..7], &16.5f32.to_be_bytes());
        assert_eq!(&data[7..11], &(-3.5f32).to_be_bytes());
        assert_eq!(&data[11..15], &3.5f32.to_be_bytes());
        // 17 | 1 << 12
        assert_eq!(&data[15..19], &4113i32.to_be_bytes());
        assert_eq!(data.len(), 19);
    }
}
