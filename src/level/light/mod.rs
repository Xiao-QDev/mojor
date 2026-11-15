// 光照系统 移植 vanilla 26.3 的 LightEngine（源列高度图 + 双队列增量 BFS）
// 相比旧的按列垂直传播：补上横向传播（洞口透光）与方块光（火把点亮）
// 语义对齐 vanilla：地表不透明方块自身格光照为 0 光存在它上面的空气格
// 异步化：快照在 region 线程抓取 计算放 tokio worker 回执 LightDone 写回
// 下发门槛：区块 lit=false 不下发 等 LightDone 回填之后才重新进下发队列
pub mod engine;
pub mod opacity;
pub mod service;
pub mod snapshot;
pub mod sources;

pub use service::{BlockChange, LightKind, LightService};
pub(crate) use service::LightJob;
pub use snapshot::{LightReceipt, Snapshot};
