// 显示世界种子 照搬 Nukkit SeedCommand 的语义
// 权限系统暂未实现 所有在线玩家都可执行

#[derive(Eq, PartialEq, Debug)]
pub enum CommandSeed {
    Seed,
}

impl CommandSeed {
    // 执行逻辑归到本模块 直接按种子拼装文案
    pub fn execute(&self, seed: u32) -> Vec<String> {
        vec![format!("Seed: {seed}")]
    }
}
