// 方块抽象 照搬 Nukkit 的 Block 基类
// 每个方块一个文件(目录里 199 个) 都 impl 这个 trait
// 属性判定(is_solid/is_transparent/is_replaceable)默认委托给 level::generator::blocks 的既有表
// 与 world 层保持一致 避免两套判定分裂
// 注意: 这里只用方法不用关联常量 否则 trait 不是 dyn 兼容(get_block 要返回 dyn Block)
// 行为(放置/破坏/掉落/tick/红石)先留默认 后续逐个方块补

use crate::level::generator::blocks;

pub trait Block {
    /// 方块运行时 id 与 level::generator::blocks 的常量一致
    fn id(&self) -> u8;
    /// 方块英文名 客户端/调试用
    fn name(&self) -> &'static str;

    /// 是否阻挡移动与实体碰撞 默认走 blocks 表的 is_solid
    fn is_solid(&self) -> bool {
        blocks::is_solid(self.id())
    }
    /// 是否透光(阳光可穿过继续向下) 默认走 blocks 表
    fn is_transparent(&self) -> bool {
        blocks::is_transparent(self.id())
    }
    /// 是否可被覆盖(树生成/放置时替换) 默认走 blocks 表
    fn is_replaceable(&self) -> bool {
        blocks::is_overridable(self.id())
    }

    // ---- 以下行为后续补 先统一默认 ----
    /// 破坏后掉落的方块 id 列表 默认掉落自身
    fn get_drops(&self) -> Vec<u8> {
        vec![self.id()]
    }
}
