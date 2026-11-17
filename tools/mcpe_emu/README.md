# mcpe_emu：用原版 libminecraftpe.so 对拍 vanilla 生成器

在 unicorn 里加载 **x86（i386）版** MCPE 0.14 的 `libminecraftpe.so`（带完整动态符号），
直接调用原版函数取真值，与 `src/level/generator/vanilla/` 的 Rust 实现逐位比较。
`.so` 本身不在仓库里，用环境变量 `MCPE_SO` 指定路径。

依赖：`pip install unicorn capstone pyelftools numpy`

| 脚本 | 对拍对象 | Rust 侧工具 |
|---|---|---|
| `heights.py <seed> <cx> <cz>`（stdin 100 个 biome） | `RandomLevelSource::getHeights` 5×5×17 密度 | `dens_dump` |
| `surf.py <seed> <cx> <cz>` | `PerlinSimplexNoise::getRegion` 地表噪声 | `surf_dump` |
| `surfcheck.py <seed> <cx> <cz> <dir>` | 各 biome 的 `buildSurfaceAt`（地表/基岩） | `surface_dump` |
| `layers2.py <seed> <0\|1> <cx0> <cz0> <n>` | `Layer::getDefaultLayers` 主层 10×10 / voronoi 16×16 | `gl_chunks` |
| `tree2.py <seed> <x> <z> <w> <h>` | 整棵 biome 层树逐节点输出 | `gl_tree` |
| `btab.py` / `biome_table.py` | 导出 biome 参数表（surface.rs / terrain.rs 里的表） | — |

要点：
- `emu.Emu()` 解析 ELF、处理重定位、给导入函数打桩；`run_init_array()` 跑静态初始化
  （`BiomeHeight`、`SimplexNoise::F2/G2` 等常量在这里写入）。
- `fakeblocks.install(e)` 给 `Block::m*` 装上只含 id 的假方块（`Block::initBlocks` 在模拟器里跑不完）。
- 生物群系对象要先 `patch_ret_this(Biome::setName)` 再调用 `Biome::initBiomes`。
- 原版 Zoom 层用定长栈数组，只能按游戏自身的窗口大小（10×10 / 16×16）请求。
