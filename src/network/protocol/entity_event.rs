// 设置血量 0xb0: pid(u8) | health(i32 BE)  原版 getId()=34 线上 0xb0
// 实体事件 0xa4: pid(u8) | eid(i64 BE) | event(u8)  原版 getId()=22 线上 0xa4
// event=2 是 HURT_ANIMATION 砸落伤害用
use bytes::Bytes;

use crate::network::protocol::pid::{ENTITY_EVENT, SET_HEALTH};
use crate::utils::binary::BinaryWriter;

pub const ENTITY_EVENT_HURT: u8 = 2;

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
