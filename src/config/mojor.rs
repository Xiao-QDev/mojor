// mojor.toml 高级配置 特性开关与性能调配
use serde::Deserialize;

// 默认值与各模块内置常量保持一致 0 一般表示按可用逻辑处理器(vCPU)数
// 部分字段已接入 部分仍用内置默认值 接入后逐个生效
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
#[allow(dead_code)]
pub struct MoJorConfig {
    // 后台区块加载线程数 0 表示按可用逻辑处理器(vCPU)数 已接入
    pub io_workers: usize,
    // tokio worker 线程数 0 表示按 vCPU 目前通过 TOKIO_WORKER_THREADS 生效
    pub tokio_workers: usize,
    // 区块离开视野后再等多少个 tick 才卸载
    pub unload_delay_ticks: u64,
    // 每 tick 最多卸载多少个区块 压住大批量卸载的抖动
    pub max_unload_per_tick: usize,
    // 每 tick 最多编码写出多少个脏块
    pub max_flush_per_tick: usize,
    // 每多少个 tick 触发一次周期保存
    pub save_interval_ticks: u64,
    // 后台刷盘间隔 秒
    pub sync_interval_secs: u64,
    // 分片连续空闲多少个 tick 后上报回收
    pub idle_shutdown_ticks: u64,
    // 是否开启加载圈预取
    pub prefetch_enabled: bool,
    // tracing 过滤表达式 如 mojor=debug,info
    pub log_level: String,
}

impl Default for MoJorConfig {
    fn default() -> Self {
        Self {
            io_workers: 0,
            tokio_workers: 0,
            unload_delay_ticks: 40,
            max_unload_per_tick: 96,
            max_flush_per_tick: 16,
            save_interval_ticks: 200,
            sync_interval_secs: 12,
            idle_shutdown_ticks: 40,
            prefetch_enabled: true,
            log_level: "mojor=info".into(),
        }
    }
}

impl MoJorConfig {
    fn cpu_count() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }

    // 0 表示跟着可用逻辑处理器(vCPU)数走 上限固定为该数 避免超配争抢
    pub fn effective_io_workers(&self) -> usize {
        let cpu = Self::cpu_count();
        if self.io_workers == 0 {
            cpu
        } else {
            self.io_workers.min(cpu)
        }
    }

    // 0 表示跟着可用逻辑处理器(vCPU)数走
    pub fn effective_tokio_workers(&self) -> usize {
        let cpu = Self::cpu_count();
        if self.tokio_workers == 0 {
            cpu
        } else {
            self.tokio_workers
        }
    }
}
