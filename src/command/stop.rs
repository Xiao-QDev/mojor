// 停止服务器 照搬 Nukkit StopCommand 语义
// 控制台与玩家均可执行 触发主循环优雅停机(先保存世界再退出)
// 权限系统暂未实现 所有在线玩家都可执行

#[derive(Eq, PartialEq, Debug)]
pub enum CommandStop {
    Stop,
}

impl CommandStop {
    pub fn message(&self) -> &'static str {
        "Stopping the server..."
    }
}
