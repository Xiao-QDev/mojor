// 天空源列 对齐 vanilla ChunkSkyLightSources 的语义
// 每列记录一个"最低源 y"：y >= 该值的格子都是天空源（直填 15）
// 隔断判定只有 dampening != 0 —— 0.14 没有 faceShapeOccludes
// 返回值域 -1..128：-1 表示整列都透光（源一直延伸到世界底下）
//   128 表示最顶上一格就隔断（整列无源）
use super::opacity::dampening;

pub const WORLD_BOTTOM_SOURCE: i16 = -1;

// 自上而下扫一列 找到第一个"下邻方块隔断"的位置
// vanilla findLowestSourceY：topState 从世界顶上方一格的空气开始
// bottomState 走到 dampening != 0 的方块时 源列停在当时的 topState 上
pub fn column_lowest_source(get_block: impl Fn(i32) -> u8) -> i16 {
    let mut top_y = 128i32;
    for y in (0..128).rev() {
        if dampening(get_block(y)) != 0 {
            return top_y as i16;
        }
        top_y = y;
    }
    WORLD_BOTTOM_SOURCE
}

#[cfg(test)]
mod tests {
    use super::*;

    // 128 格的测试列 下标是 y
    fn of_column(column: &[u8; 128]) -> impl Fn(i32) -> u8 + '_ {
        move |y| {
            if (0..128).contains(&y) {
                column[y as usize]
            } else {
                0
            }
        }
    }

    #[test]
    fn empty_column_sources_reach_below_the_world() {
        let column = [0u8; 128];
        assert_eq!(column_lowest_source(of_column(&column)), -1);
    }

    #[test]
    fn solid_ground_stops_the_source_column() {
        // y0 是石头 其上全空气：源从 y1 开始
        let mut column = [0u8; 128];
        column[0] = 1;
        assert_eq!(column_lowest_source(of_column(&column)), 1);
    }

    #[test]
    fn water_cuts_the_column_at_its_top() {
        // y40..42 是水 y43 起空气：隔断边在 y43 与水之间 源列从 y43 开始
        let mut column = [0u8; 128];
        for y in 40..=42 {
            column[y] = 8;
        }
        assert_eq!(column_lowest_source(of_column(&column)), 43);
    }

    #[test]
    fn leaves_also_cut_the_column() {
        let mut column = [0u8; 128];
        column[10] = 18; // LEAVES
        assert_eq!(column_lowest_source(of_column(&column)), 11);
    }

    #[test]
    fn glass_and_plants_do_not_cut() {
        let mut column = [0u8; 128];
        column[50] = 20; // GLASS
        column[51] = 31; // TALL_GRASS
        assert_eq!(column_lowest_source(of_column(&column)), -1);
    }

    #[test]
    fn solid_cap_makes_the_column_sourceless() {
        let mut column = [0u8; 128];
        column[127] = 1;
        assert_eq!(column_lowest_source(of_column(&column)), 128);
    }
}
