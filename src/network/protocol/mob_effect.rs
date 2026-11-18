// 生物效果包 0xa5 与 ITX MobEffectPacket 一致
// pid(u8) | eid(i64 BE) | event(u8) | effectId(u8) | amplifier(u8) | particles(u8) | duration(i32 BE)
use bytes::Bytes;

use crate::network::protocol::pid::MOB_EFFECT;
use crate::utils::binary::BinaryWriter;

pub const EVENT_ADD: u8 = 1;
pub const EVENT_MODIFY: u8 = 2;
pub const EVENT_REMOVE: u8 = 3;

#[derive(Debug, Clone, Copy)]
pub struct MobEffectPacket {
    pub eid: i64,
    pub event: u8,
    pub effect_id: u8,
    pub amplifier: u8,
    pub particles: bool,
    pub duration: i32,
}

impl MobEffectPacket {
    pub fn add(eid: i64, effect_id: u8, amplifier: u8, duration_ticks: i32) -> Self {
        Self { eid, event: EVENT_ADD, effect_id, amplifier, particles: true, duration: duration_ticks }
    }

    pub fn remove(eid: i64, effect_id: u8) -> Self {
        Self { eid, event: EVENT_REMOVE, effect_id, amplifier: 0, particles: false, duration: 0 }
    }

    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(19);
        w.put_u8(MOB_EFFECT);
        w.put_i64(self.eid);
        w.put_u8(self.event);
        w.put_u8(self.effect_id);
        w.put_u8(self.amplifier);
        w.put_u8(if self.particles { 1 } else { 0 });
        w.put_i32(self.duration);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_layout() {
        let data = MobEffectPacket::add(7, 1, 0, 600).encode();
        assert_eq!(data[0], MOB_EFFECT);
        assert_eq!(&data[1..9], &7i64.to_be_bytes());
        assert_eq!(data[9], EVENT_ADD);
        assert_eq!(data[10], 1);
        assert_eq!(data[11], 0);
        assert_eq!(data[12], 1); // particles
        assert_eq!(&data[13..17], &600i32.to_be_bytes());
        assert_eq!(data.len(), 17);
    }
}
