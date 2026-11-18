// 设置血量 0xb0: pid(u8) | health(i32 BE)  原版 getId()=34 线上 0xb0
// 实体事件 0xa4: pid(u8) | eid(i64 BE) | event(u8)  原版 getId()=22 线上 0xa4
// event=2 是 HURT_ANIMATION 砸落伤害用
use bytes::Bytes;

use crate::error::{Error, Result};
use crate::network::protocol::pid::{ENTITY_EVENT, SET_HEALTH};
use crate::utils::binary::{BinaryReader, BinaryWriter};

pub const ENTITY_EVENT_HURT: u8 = 2;
// 客户端吃完食物后上报 event=9 (EntityEventPacket::USE_ITEM) ITX/原版一致
pub const ENTITY_EVENT_USE_ITEM: u8 = 9;

pub struct SetHealthPacket {
    pub health: i32,
}

impl SetHealthPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(5);
        w.put_u8(SET_HEALTH);
        w.put_i32(self.health);
        Bytes::from(w.freeze())
    }
}

pub struct EntityEventPacket {
    pub eid: i64,
    pub event: u8,
}

impl EntityEventPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(10);
        w.put_u8(ENTITY_EVENT);
        w.put_i64(self.eid);
        w.put_u8(self.event);
        Bytes::from(w.freeze())
    }

    // 入站吃食物事件 0xa4 | eid(i64 BE) | event(u8)
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ENTITY_EVENT {
            return Err(Error::BadPacketId(pid));
        }
        Ok(Self {
            eid: r.get_i64()?,
            event: r.get_u8()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_health_layout() {
        let data = SetHealthPacket { health: 17 }.encode();
        assert_eq!(data[0], SET_HEALTH);
        assert_eq!(&data[1..5], &17i32.to_be_bytes());
        assert_eq!(data.len(), 5);
    }

    #[test]
    fn entity_event_layout() {
        let data = EntityEventPacket {
            eid: 9,
            event: ENTITY_EVENT_HURT,
        }
        .encode();
        assert_eq!(data[0], ENTITY_EVENT);
        assert_eq!(&data[1..9], &9i64.to_be_bytes());
        assert_eq!(data[9], ENTITY_EVENT_HURT);
        assert_eq!(data.len(), 10);
    }
}
