// 方块 id 常量 直接复用 world 层的权威表 不重复定义
// level::generator::blocks 才是唯一的 id 数据源(地形生成与世界层都用它)
pub use crate::level::generator::blocks::*;
