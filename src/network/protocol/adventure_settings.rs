use bytes::Bytes;

use crate::network::protocol::pid::ADVENTURE_SETTINGS;
use crate::utils::binary::BinaryWriter;

// 0x40 自动跳跃 0x80 允许飞行
pub const FLAG_AUTO_JUMP: i32 = 0x40;
pub const FLAG_ALLOW_FLIGHT: i32 = 0x80;
pub const PERMISSION_MEMBER: i32 = 0x2;

#[derive(Debug, Clone, Copy)]
pub struct AdventureSettingsPacket {
    pub flags: i32,
    pub user_permission: i32,
    pub global_permission: i32,
}

impl AdventureSettingsPacket {
    // 0xbc | flags(i32) | userPermission(i32) | globalPermission(i32)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(ADVENTURE_SETTINGS);
        w.put_i32(self.flags);
        w.put_i32(self.user_permission);
        w.put_i32(self.global_permission);
        Bytes::from(w.freeze())
    }
}
