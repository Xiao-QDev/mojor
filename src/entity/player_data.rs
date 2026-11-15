// 玩家数据存档 与 Java 版一样按 uuid 落到 <世界目录>/playerdata/<uuid>.dat
// 用户名不参与路径 换台设备用同名登录拿不到这份数据
// 早期版本按用户名存在 players/<name>.dat 升级后第一次登录时迁移到 uuid 名下 只迁一次
// mojor 没有 NBT 实现 这里用等价的定长二进制格式 字段与 Nukkit saveNBT 对齐:
// Pos / Rotation / Inventory / 快捷栏映射
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::{Error, Result};
use crate::network::protocol::item::ItemStack;
use crate::utils::binary::{BinaryReader, BinaryWriter};

use super::player::PlayerState;

// 2: 头部带 16 字节 uuid 与自己不匹配的存档一律不读
const VERSION: u8 = 2;
// 旧版按用户名存的格式 没有 uuid 头 只用于一次性迁移
const LEGACY_VERSION: u8 = 1;
const INVENTORY_SLOTS: usize = 27;
// 与 PlayerState 一致 0.14 的 ContainerSetContent 快捷栏映射是 36 项
const HOTBAR_SLOTS: usize = 36;
// Java 版的存档目录名
const DATA_DIR: &str = "playerdata";
// 早期版本放玩家存档的目录 只用于迁移
const LEGACY_DIR: &str = "players";

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerData {
    pub position: (f32, f32, f32),
    pub yaw: f32,
    pub pitch: f32,
    pub inventory: Vec<Option<ItemStack>>,
    pub hotbar: Vec<i32>,
}

impl Default for PlayerData {
    fn default() -> Self {
        Self {
            position: (0.0, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            inventory: vec![None; INVENTORY_SLOTS],
            hotbar: (0..HOTBAR_SLOTS as i32).map(|slot| slot + 36).collect(),
        }
    }
}

impl PlayerData {
    pub fn from_state(state: &PlayerState) -> Self {
        Self {
            position: state.position,
            yaw: state.yaw,
            pitch: state.pitch,
            inventory: state.inventory.clone(),
            hotbar: state.hotbar.clone(),
        }
    }

    // 读档后覆盖出生点与背包 位置朝向也恢复
    pub fn apply_to(&self, state: &mut PlayerState) {
        state.position = self.position;
        state.yaw = self.yaw;
        state.pitch = self.pitch;
        state.inventory = self.inventory.clone();
        state.hotbar = self.hotbar.clone();
    }

    pub fn encode(&self, uuid: Uuid) -> Vec<u8> {
        let mut w = BinaryWriter::with_capacity(
            1 + 16 + 5 * 4 + 1 + self.inventory.len() * 6 + 1 + self.hotbar.len() * 4,
        );
        w.put_u8(VERSION);
        w.put_bytes(uuid.as_bytes());
        w.put_f32(self.position.0);
        w.put_f32(self.position.1);
        w.put_f32(self.position.2);
        w.put_f32(self.yaw);
        w.put_f32(self.pitch);

        w.put_u8(self.inventory.len() as u8);
        for slot in &self.inventory {
            match slot {
                Some(item) if !item.is_empty() => {
                    w.put_u8(1);
                    // i16 用 u16 位模式存 读写都按同一宽度
                    w.put_u16(item.id as u16);
                    w.put_u8(item.count);
                    w.put_u16(item.damage as u16);
                }
                _ => w.put_u8(0),
            }
        }

        w.put_u8(self.hotbar.len() as u8);
        for slot in &self.hotbar {
            w.put_i32(*slot);
        }
        w.freeze().to_vec()
    }

    // 存档头部记着自己的 uuid 与预期不符说明文件名被改过或者拿错了别人的档
    pub fn decode(buf: &[u8], uuid: Uuid) -> Option<Self> {
        let mut r = BinaryReader::new(buf);
        match r.get_u8().ok()? {
            VERSION => {
                let stored = r.get_uuid().ok()?;
                if stored != *uuid.as_bytes() {
                    return None;
                }
            }
            // 旧格式没有 uuid 头 只可能出现在迁移路径上
            LEGACY_VERSION => {}
            _ => return None,
        }
        let position = (
            r.get_f32().ok()?,
            r.get_f32().ok()?,
            r.get_f32().ok()?,
        );
        let yaw = r.get_f32().ok()?;
        let pitch = r.get_f32().ok()?;

        let inv_len = r.get_u8().ok()? as usize;
        if inv_len > INVENTORY_SLOTS {
            return None;
        }
        let mut inventory = Vec::with_capacity(INVENTORY_SLOTS);
        for _ in 0..inv_len {
            if r.get_u8().ok()? == 0 {
                inventory.push(None);
            } else {
                let id = r.get_u16().ok()? as i16;
                let count = r.get_u8().ok()?;
                let damage = r.get_u16().ok()? as i16;
                inventory.push(Some(ItemStack {
                    id,
                    count,
                    damage,
                }));
            }
        }
        inventory.resize(INVENTORY_SLOTS, None);

        let hotbar_len = r.get_u8().ok()? as usize;
        if hotbar_len > HOTBAR_SLOTS {
            return None;
        }
        let mut hotbar = Vec::with_capacity(HOTBAR_SLOTS);
        for _ in 0..hotbar_len {
            hotbar.push(r.get_i32().ok()?);
        }
        hotbar.resize(HOTBAR_SLOTS, -1);

        Some(Self {
            position,
            yaw,
            pitch,
            inventory,
            hotbar,
        })
    }

    // 文件名就是 uuid 的 8-4-4-4-12 文本形式 与 Java 版 playerdata 目录一致
    fn path_of(dir: &Path, uuid: Uuid) -> PathBuf {
        dir.join(DATA_DIR).join(format!("{uuid}.dat"))
    }

    // 早期版本存档名就是用户名 只留字母数字下划线 挡住 .. 之类的路径穿越
    fn legacy_path_of(dir: &Path, name: &str) -> Option<PathBuf> {
        if name.is_empty()
            || name.len() > 32
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        Some(dir.join(LEGACY_DIR).join(format!("{name}.dat")))
    }

    // 上一版把 uuid 存档放在 players/ 目录下 文件名与现在相同 直接搬家
    fn old_uuid_path_of(dir: &Path, uuid: Uuid) -> PathBuf {
        dir.join(LEGACY_DIR).join(format!("{uuid}.dat"))
    }

    // 没有存档返回 None 由调用方退回出生点
    // 依次找 playerdata/<uuid>.dat players/<uuid>.dat players/<name>.dat 找到就迁到新位置
    pub fn load(dir: &Path, uuid: Uuid, name: &str) -> Option<Self> {
        if let Ok(buf) = std::fs::read(Self::path_of(dir, uuid)) {
            return Self::decode(&buf, uuid);
        }
        let old_uuid = Self::old_uuid_path_of(dir, uuid);
        if let Ok(buf) = std::fs::read(&old_uuid) {
            return Self::adopt(dir, &old_uuid, &buf, uuid);
        }
        let legacy = Self::legacy_path_of(dir, name)?;
        let buf = std::fs::read(&legacy).ok()?;
        Self::adopt(dir, &legacy, &buf, uuid)
    }

    // 读到旧位置的存档就写到新位置再把旧文件删掉 只迁一次
    fn adopt(dir: &Path, from: &Path, buf: &[u8], uuid: Uuid) -> Option<Self> {
        let data = Self::decode(buf, uuid)?;
        if data.save(dir, uuid).is_err() {
            return None;
        }
        let _ = std::fs::remove_file(from);
        Some(data)
    }

    // 先写临时文件再原子改名 写一半崩溃也不会留下坏档
    pub fn save(&self, dir: &Path, uuid: Uuid) -> Result<()> {
        let path = Self::path_of(dir, uuid);
        std::fs::create_dir_all(dir.join(DATA_DIR))
            .map_err(|e| Error::Storage(format!("创建 {DATA_DIR} 目录失败: {e}")))?;
        let tmp = path.with_extension("dat.tmp");
        std::fs::write(&tmp, self.encode(uuid))
            .map_err(|e| Error::Storage(format!("写入玩家数据失败: {e}")))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| Error::Storage(format!("提交玩家数据失败: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: Uuid = Uuid::from_bytes([0x11; 16]);
    const BOB: Uuid = Uuid::from_bytes([0x22; 16]);

    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mojor-player-data-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn sample() -> PlayerData {
        let mut data = PlayerData {
            position: (1.5, 65.0, -3.5),
            yaw: 90.0,
            pitch: -12.5,
            ..PlayerData::default()
        };
        data.inventory[0] = Some(ItemStack::new(2, 3));
        data.inventory[2] = Some(ItemStack {
            id: 5,
            count: 1,
            damage: 2,
        });
        data.hotbar[0] = 9;
        data
    }

    #[test]
    fn encode_decode_roundtrip() {
        let data = sample();
        let buf = data.encode(ALICE);
        assert_eq!(PlayerData::decode(&buf, ALICE), Some(data));
    }

    // 存档路径与 Java 版一致 playerdata/<uuid>.dat
    #[test]
    fn save_path_matches_java_playerdata() {
        let dir = temp_dir("path");
        sample().save(&dir, ALICE).unwrap();
        let path = dir
            .join("playerdata")
            .join("11111111-1111-1111-1111-111111111111.dat");
        assert!(path.is_file(), "应写到 {path:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 存档缺失返回 None 由登录流程退回出生点
    #[test]
    fn save_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let data = sample();
        data.save(&dir, ALICE).unwrap();
        assert_eq!(PlayerData::load(&dir, ALICE, "alice"), Some(data));
        assert_eq!(PlayerData::load(&dir, BOB, "alice"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 同一个用户名换一个 uuid 登录 拿不到别人的存档
    #[test]
    fn same_name_different_uuid_gets_separate_data() {
        let dir = temp_dir("separate");
        let alice = PlayerData {
            position: (8.0, 12.0, 8.0),
            ..PlayerData::default()
        };
        alice.save(&dir, ALICE).unwrap();
        // 别人的档检查一下 名字相同也不该读到
        assert_eq!(PlayerData::load(&dir, BOB, "alice"), None);
        alice.save(&dir, BOB).unwrap();
        let bob = PlayerData::load(&dir, BOB, "alice").unwrap();
        assert!((bob.position.0 - 8.0).abs() < 0.01);
        // 两份存档互不影响
        let back = PlayerData::load(&dir, ALICE, "alice").unwrap();
        assert!((back.position.0 - 8.0).abs() < 0.01);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 旧版本按用户名存的档 第一次登录迁移到 playerdata/<uuid>.dat
    #[test]
    fn legacy_name_save_is_migrated() {
        let dir = temp_dir("legacy");
        let data = sample();
        // 手工造一份 v1 存档 只有版本号后半段
        let legacy_bytes = {
            let full = data.encode(ALICE);
            let mut bytes = vec![LEGACY_VERSION];
            bytes.extend_from_slice(&full[17..]);
            bytes
        };
        let players = dir.join("players");
        std::fs::create_dir_all(&players).unwrap();
        std::fs::write(players.join("alice.dat"), &legacy_bytes).unwrap();

        let loaded = PlayerData::load(&dir, ALICE, "alice").expect("旧存档应能读出来");
        assert_eq!(loaded, data);
        // 迁移后落到新目录 旧文件不再存在
        assert!(PlayerData::path_of(&dir, ALICE).is_file());
        assert!(!players.join("alice.dat").exists());
        // 再读一次走的是新位置的存档
        assert_eq!(PlayerData::load(&dir, ALICE, "alice"), Some(data));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 上一版把 uuid 存档放在 players/ 下 也要搬到 playerdata/ 上来
    #[test]
    fn players_dir_uuid_save_is_migrated() {
        let dir = temp_dir("moved");
        let data = sample();
        let old = dir.join("players");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join(format!("{ALICE}.dat")), data.encode(ALICE)).unwrap();

        assert_eq!(PlayerData::load(&dir, ALICE, "alice"), Some(data));
        assert!(PlayerData::path_of(&dir, ALICE).is_file());
        assert!(!old.join(format!("{ALICE}.dat")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 迁移旧档时仍然拒绝路径穿越的用户名
    #[test]
    fn legacy_lookup_rejects_unsafe_names() {
        assert!(PlayerData::legacy_path_of(Path::new("/tmp"), "../evil").is_none());
        assert!(PlayerData::legacy_path_of(Path::new("/tmp"), "a/b").is_none());
        assert!(PlayerData::legacy_path_of(Path::new("/tmp"), "alice").is_some());
    }
}
