// OP 权限名单 仅按用户名判定(大小写不敏感) 与 MCPE 0.14 登录包里的 username 对应
// 名单持久化在 op.txt 每行一个用户名 # 开头的是注释 由 /op /deop 指令或控制台维护
// 控制台(sender=None)始终拥有最高权限 不进名单 保证空名单时也能加出第一个 OP

use std::collections::HashSet;
use std::path::{Path, PathBuf};

const DEFAULT_OP_TXT: &str = concat!(
    "# Violin OP 名单 每行一个用户名 大小写不敏感\n",
    "# 可用控制台 op <玩家名> 或游戏内 /op <玩家名> 添加\n",
);

#[derive(Debug, Clone, Default)]
pub struct OpList {
    // 一律小写存储 查询时把输入也小写 实现大小写不敏感
    names: HashSet<String>,
    // None = 纯内存(测试用) 不落盘 Some = 读写该文件
    path: Option<PathBuf>,
}

impl OpList {
    pub fn new() -> Self {
        Self::default()
    }

    // 从文件加载 文件不存在就按模板创建 坏行(空行/注释)跳过
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let mut names = HashSet::new();
        if !path.exists() {
            let _ = std::fs::write(&path, DEFAULT_OP_TXT);
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                let name = line.trim();
                if name.is_empty() || name.starts_with('#') {
                    continue;
                }
                // 用户名按 ASCII 大小写折叠 0.14 玩家名只会是 ASCII
                names.insert(name.to_ascii_lowercase());
            }
        }
        Self {
            names,
            path: Some(path),
        }
    }

    pub fn is_op(&self, name: &str) -> bool {
        self.names.contains(&name.to_ascii_lowercase())
    }

    // 增删一名 OP 返回是否真的发生了变化 有变化时写回文件
    pub fn set(&mut self, name: &str, op: bool) -> bool {
        let key = name.trim().to_ascii_lowercase();
        if key.is_empty() {
            return false;
        }
        let changed = if op {
            self.names.insert(key)
        } else {
            self.names.remove(&key)
        };
        if changed {
            let _ = self.save();
        }
        changed
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut lines = vec![
            "# Violin OP 名单 每行一个用户名 大小写不敏感".to_string(),
        ];
        lines.extend(self.names_sorted().into_iter().map(str::to_string));
        lines.push(String::new());
        std::fs::write(path, lines.join("\n"))
    }

    // 排序后的小写名单 供 /ops 展示 输出稳定
    pub fn names_sorted(&self) -> Vec<&str> {
        let mut sorted: Vec<&str> = self.names.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_names_are_case_insensitive() {
        let mut ops = OpList::new();
        assert!(ops.set("Alice", true));
        assert!(ops.is_op("alice"));
        assert!(ops.is_op("ALICE"));
        assert!(ops.is_op("Alice"));
        assert!(!ops.is_op("bob"));
    }

    #[test]
    fn set_reports_change_and_toggles() {
        let mut ops = OpList::new();
        assert!(ops.set("bob", true));
        assert!(!ops.set("bob", true), "重复添加不算变化");
        assert_eq!(ops.names_sorted(), vec!["bob"]);
        assert!(ops.set("BOB", false));
        assert!(!ops.set("bob", false), "重复删除不算变化");
        assert!(ops.is_empty());
        assert!(!ops.set("  ", true), "空白用户名忽略");
    }

    #[test]
    fn loads_skips_comments_and_persists_sorted() {
        let dir = std::env::temp_dir().join(format!("violin-op-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("op.txt");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "# comment\n\nAlice\n  bob  \n").unwrap();

        let mut ops = OpList::load(&path);
        assert_eq!(ops.names_sorted(), vec!["alice", "bob"]);
        assert!(ops.set("Carol", true));
        // 重新加载确认落盘 且注释行被跳过
        let again = OpList::load(&path);
        assert_eq!(again.names_sorted(), vec!["alice", "bob", "carol"]);
        assert!(again.is_op("CAROL"));

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Violin OP"), "重写后保留表头注释: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
