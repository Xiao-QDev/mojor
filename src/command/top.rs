use crate::tick::metrics::{TARGET_TPS, TickStats};
use crate::tick::os::{MAX_CORES, OsSample};

const CPU_BUSY: f32 = 60.0;
const CPU_WARN: f32 = 85.0;
const MEM_BUSY: f32 = 70.0;
const MEM_WARN: f32 = 90.0;

#[derive(Eq, PartialEq, Debug)]
pub enum TopCommand {
    Top,
}

impl TopCommand {
    pub fn execute(&self, os: &OsSample, ticks: Option<TickStats>) -> Vec<String> {
        let mut lines = vec![
            "§f----------- §6§lMojor Top §f-----------§r".to_string(),
            format!(
                "§fUptime §a{}§f  Threads {}§f  Cores §a{}",
                uptime(os.uptime_secs),
                count(os.threads),
                os.core_count
            ),
        ];

        if !os.ready {
            lines.push("§8sampling…".to_string());
        }

        lines.push("§bCPU".to_string());
        if os.ready {
            lines.push(format!(
                "§f  total {}§f  load {}§7 (1/5/15m)",
                pct(os.cpu_total, CPU_BUSY, CPU_WARN),
                load(os.load)
            ));
            lines.push(format!(
                "§f  proc {}§f  RSS §a{}§f  VSZ §a{}",
                pct(os.proc_cpu, CPU_BUSY, CPU_WARN),
                mb(os.rss_mb),
                mb(os.vsz_mb)
            ));
            for line in cores(os) {
                lines.push(format!("§7  {line}"));
            }
        } else {
            lines.push("§8  warming up…".to_string());
        }

        lines.push("§bMEM".to_string());
        lines.push(format!(
            "§f  used {}§f / §a{}§f  avail §a{}",
            pct(os.mem_used_pct(), MEM_BUSY, MEM_WARN),
            mb(os.mem_total_mb),
            mb(os.mem_avail_mb)
        ));
        if os.swap_total_mb > 0 {
            lines.push(format!(
                "§f  swap §a{}§f / {}",
                mb(os.swap_used_mb),
                mb(os.swap_total_mb)
            ));
        }

        lines.push("§bTICK".to_string());
        match ticks {
            Some(stats) => {
                let color = if stats.tps >= TARGET_TPS - 0.5 { "§a" } else { "§c" };
                lines.push(format!(
                    "§f  {color}{:.2} TPS§f  MSPT §a{:.2}/{:.2} ms§f  lag §a{:.0} ms",
                    stats.tps, stats.mspt_avg, stats.mspt_max, stats.lag_ms
                ));
            }
            None => lines.push("§f  TPS §8no sample yet".to_string()),
        }

        lines
    }
}

fn cores(os: &OsSample) -> Vec<String> {
    const PER_LINE: usize = 4;
    let shown = os.core_count.min(MAX_CORES);
    let mut out = Vec::new();
    for start in (0..shown).step_by(PER_LINE) {
        let end = (start + PER_LINE).min(shown);
        let mut line = String::new();
        for index in start..end {
            if index > start {
                line.push_str("  ");
            }
            line.push_str(&format!(
                "§7core{index} {}",
                pct(os.cores[index], CPU_BUSY, CPU_WARN)
            ));
        }
        out.push(line);
    }
    if os.core_count > shown
        && let Some(last) = out.last_mut()
    {
        last.push_str(&format!("  §7…+{}", os.core_count - shown));
    }
    out
}

fn pct(value: f32, busy: f32, warn: f32) -> String {
    let color = if value >= warn {
        "§c"
    } else if value >= busy {
        "§e"
    } else {
        "§a"
    };
    format!("{color}{value:.1}%")
}

// 拿不到就显示 NA 不要用 0 冒充正常值
fn count(value: Option<usize>) -> String {
    match value {
        Some(value) => format!("§a{value}"),
        None => "§8NA".to_string(),
    }
}

// Windows 没有 load average 同样显示 NA
fn load(value: Option<(f64, f64, f64)>) -> String {
    match value {
        Some((one, five, fifteen)) => format!("§a{one:.2} {five:.2} {fifteen:.2}"),
        None => "§8NA".to_string(),
    }
}

fn mb(value: u64) -> String {
    if value >= 1024 {
        format!("{:.1} GB", value as f32 / 1024.0)
    } else {
        format!("{value} MB")
    }
}

fn uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{mins}m")
    } else {
        format!("{mins}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cpu: f32, mem_used: u64, mem_total: u64) -> OsSample {
        let mut sample = OsSample {
            cpu_total: cpu,
            core_count: 4,
            mem_used_mb: mem_used,
            mem_total_mb: mem_total,
            mem_avail_mb: mem_total.saturating_sub(mem_used),
            rss_mb: 128,
            vsz_mb: 2048,
            proc_cpu: 5.5,
            threads: Some(42),
            load: Some((0.42, 0.38, 0.31)),
            uptime_secs: 3 * 3600 + 12 * 60,
            ready: true,
            ..OsSample::default()
        };
        sample.cores[..4].copy_from_slice(&[10.0, 20.0, 30.0, 40.0]);
        sample
    }

    fn stats(tps: f64) -> TickStats {
        TickStats {
            tps,
            mspt_avg: 0.3,
            mspt_max: 0.72,
            lag_ms: 0.0,
            ticks: 1200,
            samples: 1200,
        }
    }

    #[test]
    fn prints_all_sections() {
        let lines = TopCommand::Top.execute(&sample(12.3, 512, 7936), Some(stats(20.0)));
        assert_eq!(lines[0], "§f----------- §6§lMojor Top §f-----------§r");
        assert!(lines.iter().any(|l| l == "§bCPU"));
        assert!(lines.iter().any(|l| l == "§bMEM"));
        assert!(lines.iter().any(|l| l == "§bTICK"));
        assert!(
            lines.iter().any(|l| l.contains("§a12.3%") && l.contains("load")),
            "CPU 行不对: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("§7core0 §a10.0%") && l.contains("§7core3 §a40.0%")),
            "核心行不对: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("used §a6.5%") && l.contains("§a7.8 GB") && l.contains("avail")),
            "内存行不对: {lines:?}"
        );
        assert!(lines.iter().any(|l| l.contains("§a20.00 TPS")));
    }

    #[test]
    fn missing_metrics_show_na() {
        let mut sample = sample(10.0, 1, 100);
        sample.load = None;
        sample.threads = None;
        let lines = TopCommand::Top.execute(&sample, None);
        let head = &lines[1];
        assert!(head.contains("Threads §8NA"), "{head}");
        let cpu = lines.iter().find(|l| l.contains("total")).unwrap();
        assert!(cpu.contains("load §8NA"), "{cpu}");
    }

    #[test]
    fn cores_wrap_every_four() {
        let mut sample = sample(10.0, 1, 100);
        sample.core_count = 8;
        sample.cores[..8].copy_from_slice(&[1.0; 8]);
        let lines = TopCommand::Top.execute(&sample, None);
        let core_lines: Vec<&String> = lines.iter().filter(|l| l.contains("§7core")).collect();
        assert_eq!(core_lines.len(), 2, "8 核应该折成两行: {core_lines:?}");
        assert!(core_lines[0].contains("core3") && !core_lines[0].contains("core4"));
        assert!(core_lines[1].contains("core4") && !core_lines[1].contains("core0"));
    }

    #[test]
    fn cores_over_cap_show_remainder() {
        let mut sample = sample(10.0, 1, 100);
        sample.core_count = 20;
        let lines = TopCommand::Top.execute(&sample, None);
        let core_lines: Vec<&String> = lines.iter().filter(|l| l.contains("§7core")).collect();
        assert_eq!(core_lines.len(), MAX_CORES / 4);
        assert!(core_lines.last().unwrap().contains("…+4"), "{core_lines:?}");
    }

    #[test]
    fn usage_has_three_colour_bands() {
        let mut busy = sample(70.0, 70, 100);
        busy.cores[..4].copy_from_slice(&[10.0, 65.0, 90.0, 84.9]);
        let lines = TopCommand::Top.execute(&busy, Some(stats(20.0)));
        let cpu = lines.iter().find(|l| l.contains("total")).unwrap();
        assert!(cpu.contains("§e70.0%"), "中等负载该黄: {cpu}");
        let cores = lines.iter().find(|l| l.contains("core0")).unwrap();
        assert!(cores.contains("core0 §a10.0%"), "{cores}");
        assert!(cores.contains("core1 §e65.0%"), "{cores}");
        assert!(cores.contains("core2 §c90.0%"), "{cores}");
        assert!(cores.contains("core3 §e84.9%"), "临界值仍属黄区: {cores}");
        let mem = lines.iter().find(|l| l.contains("used")).unwrap();
        assert!(mem.contains("§e70.0%"), "{mem}");
    }

    #[test]
    fn warns_on_high_cpu_and_mem() {
        let lines = TopCommand::Top.execute(&sample(95.0, 95, 100), Some(stats(18.0)));
        assert!(
            lines.iter().any(|l| l.contains("§c95.0%")),
            "高 CPU 该标红: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("§c18.00 TPS")),
            "掉帧该标红: {lines:?}"
        );
    }

    #[test]
    fn first_second_shows_warming_up() {
        let mut sample = sample(0.0, 0, 0);
        sample.ready = false;
        let lines = TopCommand::Top.execute(&sample, None);
        assert!(lines.iter().any(|l| l.contains("warming up")));
        assert!(lines.iter().any(|l| l.contains("no sample yet")));
    }

    #[test]
    fn swap_line_only_when_present() {
        let mut without = sample(1.0, 1, 2);
        without.swap_total_mb = 0;
        assert!(!TopCommand::Top.execute(&without, None).iter().any(|l| l.contains("swap")));

        let mut with = sample(1.0, 1, 2);
        with.swap_total_mb = 2048;
        with.swap_used_mb = 0;
        assert!(TopCommand::Top.execute(&with, None).iter().any(|l| l.contains("swap")));
    }

    #[test]
    fn lists_every_core() {
        let mut sample = sample(10.0, 1, 100);
        sample.core_count = 20;
        for index in 0..20 {
            sample.cores[index] = index as f32;
        }
        let lines = TopCommand::Top.execute(&sample, None);
        let core_lines: Vec<&String> = lines.iter().filter(|l| l.contains("§7core")).collect();
        assert_eq!(core_lines.len(), 5, "{core_lines:?}");
        assert!(core_lines[0].contains("core0") && core_lines[0].contains("core3"));
        assert!(core_lines[4].contains("core19"));
        assert!(!core_lines[4].contains("…+"), "20 核不该截断");
    }

    #[test]
    fn truncates_only_past_buffer_cap() {
        let mut sample = sample(10.0, 1, 100);
        sample.core_count = MAX_CORES + 8;
        let lines = TopCommand::Top.execute(&sample, None);
        let core_lines: Vec<&String> = lines.iter().filter(|l| l.contains("§7core")).collect();
        assert_eq!(core_lines.len(), MAX_CORES / 4);
        assert!(core_lines.last().unwrap().contains("…+8"));
    }

    #[test]
    fn uptime_formats() {
        assert_eq!(uptime(90), "1m");
        assert_eq!(uptime(3 * 3600 + 12 * 60), "3h12m");
        assert_eq!(uptime(26 * 3600), "1d2h");
    }
}
