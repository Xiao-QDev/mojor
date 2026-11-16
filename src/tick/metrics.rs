// TPS / MSPT 采样 主循环每拍写一条 环形窗口只保留最近 60 秒
// TPS 由实际 tick 间隔反推 而不是直接报标称的 20
// 因为 ticker 用的是 MissedTickBehavior::Delay 超时只会顺延不会补帧
// 卡一下之后 tick 数是真的少 只有实测间隔能看出来

use std::collections::VecDeque;
use std::time::Duration;

// 标称 tick 间隔 与 tick_manager::TICK_MS 保持一致
pub const TICK: Duration = Duration::from_millis(50);
// 标称满速 满速跑满 60 秒的采样数就是窗口长度
pub const TARGET_TPS: f64 = 20.0;
pub const WINDOW_TICKS: usize = 1200;

#[derive(Debug, Clone, Copy)]
struct Sample {
    // 上一拍开始到这一拍开始的间隔
    gap: Duration,
    // 这一拍干活花了多久
    mspt: Duration,
}

// 窗口内的统计结果 /tps 直接打印这些字段
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickStats {
    pub tps: f64,
    pub mspt_avg: f64,
    pub mspt_max: f64,
    // 窗口内累计落后标称节奏的时间
    pub lag_ms: f64,
    // 窗口内的采样数 采样不足一分钟时小于 WINDOW_TICKS
    pub samples: usize,
    // 服务端启动以来的总 tick 数
    pub ticks: u64,
}

// 单个分片的健康快照 由 region 线程每秒上报一次
// 和主循环的 TickMeter 分开:分片卡住时主循环可能还是绿的
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionHealth {
    pub region: (i32, i32),
    // 窗口内平均每拍干活耗时
    pub mspt: f32,
    // 窗口内最慢的一拍
    pub max_mspt: f32,
    // 窗口内实际拍数 / 真实时长 掉拍会低于 20
    pub tps: f32,
    pub chunks: usize,
    pub players: usize,
    pub entities: usize,
}

impl RegionHealth {
    // 单个 tick 的预算占用比例 与健康报告里的 Utilisation 一致
    pub fn utilisation(&self) -> f32 {
        self.mspt / (TICK.as_secs_f32() * 1000.0) * 100.0
    }
}

#[derive(Debug)]
pub struct TickMeter {
    samples: VecDeque<Sample>,
    ticks: u64,
}

impl Default for TickMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl TickMeter {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(WINDOW_TICKS),
            ticks: 0,
        }
    }

    // gap 是上一拍开始到这一拍开始的间隔 mspt 是这一拍干活花了多久
    pub fn record(&mut self, gap: Duration, mspt: Duration) {
        if self.samples.len() == WINDOW_TICKS {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { gap, mspt });
        self.ticks += 1;
    }

    // 一拍都还没跑完时返回 None 让调用方自己决定怎么提示
    pub fn stats(&self) -> Option<TickStats> {
        let samples = self.samples.len();
        if samples == 0 {
            return None;
        }
        let tick_ms = TICK.as_secs_f64() * 1000.0;
        let mut gap_sum = 0.0_f64;
        let mut mspt_sum = 0.0_f64;
        let mut mspt_max = 0.0_f64;
        let mut behind = 0.0_f64;
        for sample in &self.samples {
            let gap = sample.gap.as_secs_f64() * 1000.0;
            let mspt = sample.mspt.as_secs_f64() * 1000.0;
            gap_sum += gap;
            mspt_sum += mspt;
            mspt_max = mspt_max.max(mspt);
            // 比标称节奏多花的时间才是落后 提前跑完不算
            behind += (gap - tick_ms).max(0.0);
        }
        let avg_gap = gap_sum / samples as f64;
        Some(TickStats {
            // 偶尔小于 50ms 的间隔别显示成 20.01 夹到标称值
            tps: (1000.0 / avg_gap.max(f64::MIN_POSITIVE)).min(TARGET_TPS),
            mspt_avg: mspt_sum / samples as f64,
            mspt_max,
            lag_ms: behind,
            samples,
            ticks: self.ticks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_speed() -> TickMeter {
        let mut meter = TickMeter::new();
        for _ in 0..WINDOW_TICKS {
            meter.record(TICK, Duration::from_millis(2));
        }
        meter
    }

    #[test]
    fn empty_meter_has_no_stats() {
        assert!(TickMeter::new().stats().is_none());
    }

    #[test]
    fn full_speed_reports_twenty_tps_without_lag() {
        let stats = full_speed().stats().unwrap();
        assert!((stats.tps - TARGET_TPS).abs() < 0.001);
        assert!((stats.mspt_avg - 2.0).abs() < 0.001);
        assert!((stats.mspt_max - 2.0).abs() < 0.001);
        assert_eq!(stats.lag_ms, 0.0);
        assert_eq!(stats.samples, WINDOW_TICKS);
        assert_eq!(stats.ticks, WINDOW_TICKS as u64);
    }

    #[test]
    fn slow_ticks_lower_tps_and_report_lag() {
        // 每拍实际用了 100ms 相当于掉到 10 TPS 每拍落后 50ms
        let mut meter = TickMeter::new();
        for _ in 0..10 {
            meter.record(Duration::from_millis(100), Duration::from_millis(95));
        }
        let stats = meter.stats().unwrap();
        assert!((stats.tps - 10.0).abs() < 0.001, "tps = {}", stats.tps);
        assert!((stats.mspt_avg - 95.0).abs() < 0.001);
        assert!((stats.lag_ms - 500.0).abs() < 0.001, "lag = {}", stats.lag_ms);
    }

    #[test]
    fn one_slow_tick_shows_up_in_max_not_avg() {
        let mut meter = full_speed();
        meter.record(Duration::from_millis(250), Duration::from_millis(200));
        let stats = meter.stats().unwrap();
        // 单次尖峰不该把平均值拉爆 最大值必须看得见
        assert!(stats.mspt_avg < 3.0, "avg = {}", stats.mspt_avg);
        assert!((stats.mspt_max - 200.0).abs() < 0.001);
        // 这一拍 gap 250ms 比标称多 200ms 窗口里其余各拍都不落后
        assert!((stats.lag_ms - 200.0).abs() < 0.001, "lag = {}", stats.lag_ms);
    }

    #[test]
    fn window_keeps_only_the_latest_samples() {
        let mut meter = full_speed();
        // 再塞 100 条 窗口应该只留下最新的 WINDOW_TICKS 条
        for _ in 0..100 {
            meter.record(TICK, Duration::from_millis(2));
        }
        let stats = meter.stats().unwrap();
        assert_eq!(stats.samples, WINDOW_TICKS);
        assert_eq!(stats.ticks, WINDOW_TICKS as u64 + 100);
    }
}
