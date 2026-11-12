use bytes::Bytes;

use crate::network::protocol::login::Skin;
use crate::network::protocol::pid::PLAYER_LIST;
use crate::utils::binary::BinaryWriter;

pub const TYPE_ADD: u8 = 0;
pub const TYPE_REMOVE: u8 = 1;

#[derive(Debug, Clone)]
pub struct PlayerListEntry {
    pub uuid: [u8; 16],
    pub eid: i64,
    pub username: String,
    pub skin: Skin,
}

#[derive(Debug, Clone)]
pub struct PlayerListPacket {
    pub kind: u8,
    pub entries: Vec<PlayerListEntry>,
}

impl PlayerListPacket {
    pub fn add(entries: Vec<PlayerListEntry>) -> Self {
        Self {
            kind: TYPE_ADD,
            entries,
        }
    }

    // 移除只需要 uuid 其余字段忽略
    pub fn remove(uuids: Vec<[u8; 16]>) -> Self {
        let entries = uuids
            .into_iter()
            .map(|uuid| PlayerListEntry {
                uuid,
                eid: 0,
                username: String::new(),
                skin: Skin {
                    model: String::new(),
                    data: Bytes::new(),
                },
            })
            .collect();
        Self {
            kind: TYPE_REMOVE,
            entries,
        }
    }

    // 0xc3 | type(u8) | count(i32) | [uuid | eid(i64) | name | skin]
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(PLAYER_LIST);
        w.put_u8(self.kind);
        w.put_i32(self.entries.len() as i32);

        for entry in &self.entries {
            w.put_bytes(&entry.uuid);
            if self.kind != TYPE_ADD {
                continue;
            }
            w.put_i64(entry.eid);
            w.put_string(&entry.username)
                .expect("玩家名不会超过 u16 长度");
            // 皮肤 模型名 + u16 长度 + 像素数据
            w.put_string(&entry.skin.model)
                .expect("皮肤模型名不会超过 u16 长度");
            w.put_u16(entry.skin.data.len() as u16);
            w.put_bytes(&entry.skin.data);
        }

        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> PlayerListEntry {
        PlayerListEntry {
            uuid: [0xbb; 16],
            eid: 3,
            username: "bob".into(),
            skin: Skin {
                model: "Standard_Steve".into(),
                data: Bytes::from_static(&[0x7f; 4]),
            },
        }
    }

    #[test]
    fn player_list_add_layout() {
        let data = PlayerListPacket::add(vec![entry()]).encode();
        assert_eq!(data[0], PLAYER_LIST);
        assert_eq!(data[1], TYPE_ADD);
        assert_eq!(&data[2..6], &1i32.to_be_bytes());
        assert_eq!(&data[6..22], &[0xbb; 16]);
        assert_eq!(&data[22..30], &3i64.to_be_bytes());
        assert_eq!(&data[30..32], &3u16.to_be_bytes());
        assert_eq!(&data[32..35], b"bob");
        assert_eq!(&data[35..37], &14u16.to_be_bytes());
        assert_eq!(&data[37..51], b"Standard_Steve");
        assert_eq!(&data[51..53], &4u16.to_be_bytes());
        assert_eq!(&data[53..], &[0x7f; 4]);
    }

    #[test]
    fn player_list_remove_only_carries_uuid() {
        let data = PlayerListPacket::remove(vec![[0xbb; 16]]).encode();
        assert_eq!(data[0], PLAYER_LIST);
        assert_eq!(data[1], TYPE_REMOVE);
        assert_eq!(&data[2..6], &1i32.to_be_bytes());
        assert_eq!(&data[6..], &[0xbb; 16]);
    }
}
