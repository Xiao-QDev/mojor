use bytes::Bytes;

use crate::network::protocol::pid::UPDATE_ATTRIBUTES;
use crate::utils::binary::BinaryWriter;

// 0.14 的属性名不是新版那种 minecraft:xxx
pub const MAX_HEALTH: &str = "generic.health";
pub const MOVEMENT_SPEED: &str = "generic.movementSpeed";

#[derive(Debug, Clone)]
pub struct Attribute {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct UpdateAttributesPacket {
    pub eid: i64,
    pub entries: Vec<Attribute>,
}

impl UpdateAttributesPacket {
    // 进世界时的基础属性 血量 20 移动速度 0.1
    pub fn survival(eid: i64) -> Self {
        Self {
            eid,
            entries: vec![
                Attribute {
                    min: 0.0,
                    max: 20.0,
                    value: 20.0,
                    name: MAX_HEALTH.to_string(),
                },
                Attribute {
                    min: 0.0,
                    max: 24791.0,
                    value: 0.1,
                    name: MOVEMENT_SPEED.to_string(),
                },
            ],
        }
    }

    // 0xa6 | eid(i64) | count(u16) | [min f32 | max f32 | value f32 | name]
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(UPDATE_ATTRIBUTES);
        w.put_i64(self.eid);
        w.put_u16(self.entries.len() as u16);

        for entry in &self.entries {
            w.put_f32(entry.min);
            w.put_f32(entry.max);
            w.put_f32(entry.value);
            w.put_string(&entry.name)
                .expect("属性名不会超过 u16 长度");
        }

        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_attributes_layout() {
        let data = UpdateAttributesPacket::survival(0).encode();
        assert_eq!(data[0], UPDATE_ATTRIBUTES);
        assert_eq!(&data[1..9], &0i64.to_be_bytes());
        assert_eq!(&data[9..11], &2u16.to_be_bytes());
        // generic.health
        assert_eq!(&data[11..15], &0.0f32.to_be_bytes());
        assert_eq!(&data[15..19], &20.0f32.to_be_bytes());
        assert_eq!(&data[19..23], &20.0f32.to_be_bytes());
        assert_eq!(&data[23..25], &14u16.to_be_bytes());
        assert_eq!(&data[25..39], b"generic.health");
        // generic.movementSpeed
        assert_eq!(&data[39..43], &0.0f32.to_be_bytes());
        assert_eq!(&data[43..47], &24791.0f32.to_be_bytes());
        assert_eq!(&data[47..51], &0.1f32.to_be_bytes());
        assert_eq!(&data[51..53], &21u16.to_be_bytes());
        assert_eq!(&data[53..], b"generic.movementSpeed");
    }
}
