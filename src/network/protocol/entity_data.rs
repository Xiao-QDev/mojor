// 0.14 元数据条目 首字节为 (类型 << 5) | 序号 数值一律小端
pub const DATA_FLAGS: u8 = 0;
pub const DATA_AIR: u8 = 1;
pub const DATA_NAMETAG: u8 = 2;
pub const DATA_SHOW_NAMETAG: u8 = 3;

const TYPE_BYTE: u8 = 0;
const TYPE_SHORT: u8 = 1;
const TYPE_STRING: u8 = 4;

const END: u8 = 0x7f;

#[derive(Debug, Default, Clone)]
pub struct Metadata {
    bytes: Vec<u8>,
}

impl Metadata {
    pub fn new() -> Self {
        Self::default()
    }

    // 玩家实体的最小元数据集合 与 Nukkit 的默认值对齐
    pub fn player(username: &str) -> Self {
        Self::player_with_flags(username, 0)
    }

    // 与 Nukkit 的 sendData 一致 姿态变化时发完整集合 只换标志位
    pub fn player_with_flags(username: &str, flags: u8) -> Self {
        let mut m = Self::new();
        m.put_byte(DATA_FLAGS, flags)
            .put_short(DATA_AIR, 300)
            .put_string(DATA_NAMETAG, username)
            .put_byte(DATA_SHOW_NAMETAG, 1);
        m
    }

    // 只带实体标志的增量元数据 潜行与疾跑等姿态靠它同步
    pub fn flags(value: u8) -> Self {
        let mut m = Self::new();
        m.put_byte(DATA_FLAGS, value);
        m
    }

    pub fn len(&self) -> usize {
        self.bytes.len() + 1
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn header(&mut self, type_id: u8, id: u8) {
        self.bytes.push((type_id << 5) | (id & 0x1f));
    }

    pub fn put_byte(&mut self, id: u8, value: u8) -> &mut Self {
        self.header(TYPE_BYTE, id);
        self.bytes.push(value);
        self
    }

    pub fn put_short(&mut self, id: u8, value: u16) -> &mut Self {
        self.header(TYPE_SHORT, id);
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn put_string(&mut self, id: u8, value: &str) -> &mut Self {
        self.header(TYPE_STRING, id);
        let b = value.as_bytes();
        self.bytes.extend_from_slice(&(b.len() as u16).to_le_bytes());
        self.bytes.extend_from_slice(b);
        self
    }

    // 以 0x7f 结束
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.bytes.clone();
        out.push(END);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_metadata_layout() {
        let raw = Metadata::player("mineadc").encode();
        // flags(0) = 0
        assert_eq!(raw[0], (TYPE_BYTE << 5) | DATA_FLAGS);
        assert_eq!(raw[1], 0);
        // air(1) = 300 小端
        assert_eq!(raw[2], (TYPE_SHORT << 5) | DATA_AIR);
        assert_eq!(&raw[3..5], &300u16.to_le_bytes());
        // nametag(2)
        assert_eq!(raw[5], (TYPE_STRING << 5) | DATA_NAMETAG);
        assert_eq!(&raw[6..8], &7u16.to_le_bytes());
        assert_eq!(&raw[8..15], b"mineadc");
        // show nametag(3) = 1
        assert_eq!(raw[15], (TYPE_BYTE << 5) | DATA_SHOW_NAMETAG);
        assert_eq!(raw[16], 1);
        assert_eq!(*raw.last().unwrap(), END);
        assert_eq!(raw.len(), 18);
    }
}
