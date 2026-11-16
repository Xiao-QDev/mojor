use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use sysinfo::{ProcessesToUpdate, System, get_current_pid};
pub const MAX_CORES: usize = 256;

const BYTES_PER_MB: u64 = 1024 * 1024;

const SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Debug, Clone, Copy)]
pub struct OsSample {
    pub cpu_total: f32,
    pub cores: [f32; MAX_CORES],
    pub core_count: usize,
    pub load: Option<(f64, f64, f64)>,
    pub mem_total_mb: u64,
    pub mem_used_mb: u64,
    pub mem_avail_mb: u64,
    pub swap_total_mb: u64,
    pub swap_used_mb: u64,
    pub rss_mb: u64,
    pub vsz_mb: u64,
    pub proc_cpu: f32,
    pub threads: Option<usize>,
    pub uptime_secs: u64,
    pub ready: bool,
}

impl OsSample {
    pub fn mem_used_pct(&self) -> f32 {
        if self.mem_total_mb == 0 {
            0.0
        } else {
            self.mem_used_mb as f32 * 100.0 / self.mem_total_mb as f32
        }
    }
}

pub struct OsMeter {
    sample: Arc<RwLock<OsSample>>,
}

impl OsMeter {
    pub fn new() -> Self {
        let sample = Arc::new(RwLock::new(OsSample::default()));
        let shared = Arc::clone(&sample);
        let _ = thread::Builder::new()
            .name("os-sample".to_string())
            .spawn(move || {
                let mut sys = System::new().ok();
                let mut out = OsSample::default();
                let mut refreshes = 0u32;
                loop {
                    collect(sys.as_mut(), &mut out, &mut refreshes);
                    *shared.write().unwrap_or_else(|e| e.into_inner()) = out;
                    thread::sleep(SAMPLE_INTERVAL);
                }
            });
        Self { sample }
    }

    pub fn sample(&self) -> OsSample {
        *self.sample.read().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for OsSample {
    fn default() -> Self {
        Self {
            cpu_total: 0.0,
            cores: [0.0; MAX_CORES],
            core_count: 0,
            load: None,
            mem_total_mb: 0,
            mem_used_mb: 0,
            mem_avail_mb: 0,
            swap_total_mb: 0,
            swap_used_mb: 0,
            rss_mb: 0,
            vsz_mb: 0,
            proc_cpu: 0.0,
            threads: None,
            uptime_secs: 0,
            ready: false,
        }
    }
}

fn collect(sys: Option<&mut System>, out: &mut OsSample, refreshes: &mut u32) {
    let Some(sys) = sys else {
        return;
    };

    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpus = sys.cpus();
    out.cpu_total = sys.global_cpu_usage();
    out.core_count = cpus.len().min(MAX_CORES);
    out.cores = [0.0; MAX_CORES];
    for (slot, cpu) in out.cores.iter_mut().zip(cpus.iter()) {
        *slot = cpu.usage();
    }

    out.load = System::load_average()
        .ok()
        .map(|load| (load.one, load.five, load.fifteen));
    if let Ok(uptime) = System::uptime() {
        out.uptime_secs = uptime;
    }

    out.mem_total_mb = sys.total_memory() / BYTES_PER_MB;
    out.mem_used_mb = sys.used_memory() / BYTES_PER_MB;
    out.mem_avail_mb = sys.available_memory() / BYTES_PER_MB;
    out.swap_total_mb = sys.total_swap() / BYTES_PER_MB;
    out.swap_used_mb = sys.used_swap() / BYTES_PER_MB;

    if let Ok(pid) = get_current_pid() {
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        if let Some(proc) = sys.process(pid) {
            out.rss_mb = proc.memory() / BYTES_PER_MB;
            out.vsz_mb = proc.virtual_memory() / BYTES_PER_MB;
            let cores = sys.cpus().len().max(1) as f32;
            out.proc_cpu = (proc.cpu_usage() / cores).min(100.0);
        }
    }
    out.threads = crate::utils::platform::thread_count()
        .or_else(|| get_current_pid().ok().and_then(|pid| sys.process(pid)).and_then(|proc| proc.tasks().map(|tasks| tasks.len())));

    *refreshes += 1;
    out.ready = *refreshes >= 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_sample_is_ready() {
        let mut sys = System::new().ok();
        let mut sample = OsSample::default();
        let mut refreshes = 0u32;
        collect(sys.as_mut(), &mut sample, &mut refreshes);
        assert!(!sample.ready, "第一次采样只是基线");
        collect(sys.as_mut(), &mut sample, &mut refreshes);
        assert!(sample.ready);
    }

    #[test]
    fn memory_numbers_are_plausible() {
        let mut sys = System::new().ok();
        let mut sample = OsSample::default();
        let mut refreshes = 0u32;
        collect(sys.as_mut(), &mut sample, &mut refreshes);
        assert!(sample.mem_total_mb > 0, "读不到物理内存");
        assert!(sample.mem_used_mb <= sample.mem_total_mb);
        assert!(sample.mem_used_pct() <= 100.0);
        assert!(sample.core_count > 0, "读不到 CPU 核心");
    }

    #[test]
    fn thread_count_is_never_zero_when_known() {
        // 本进程至少有主线程 拿得到就必须 >= 1
        if let Some(count) = crate::utils::platform::thread_count() {
            assert!(count >= 1, "线程数不可能为 0: {count}");
        }
    }

    #[test]
    fn meter_samples_in_background() {
        let meter = OsMeter::new();
        // 后台线程第一轮很快就写完 第一份可能还没 ready
        thread::sleep(SAMPLE_INTERVAL * 3 / 2);
        let sample = meter.sample();
        assert!(sample.core_count > 0, "后台线程没写进快照");
    }
}
