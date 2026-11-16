use crate::tick::metrics::{RegionHealth, TARGET_TPS, TickStats};

const REGION_BLOCKS: i32 = 512;

#[derive(Eq, PartialEq, Debug)]
pub enum CommandTps {
    Tps,
}

impl CommandTps {
    pub fn execute(
        &self,
        stats: Option<TickStats>,
        regions: &[RegionHealth],
        online: usize,
        world: &str,
    ) -> Vec<String> {
        let mut lines = vec![
            "§bServer Health Report".to_string(),
            format!("§7- §fOnline Players: §a{online}"),
            format!("§7- §fTotal regions: §a{}", regions.len()),
        ];

        match stats {
            Some(stats) => {
                let color = if stats.tps >= TARGET_TPS - 0.5 { "§a" } else { "§c" };
                lines.push(format!(
                    "§7- §fMain {color}{:.2} TPS§f at §a{:.2}/{:.2} ms§f, lag §a{:.0} ms",
                    stats.tps, stats.mspt_avg, stats.mspt_max, stats.lag_ms
                ));
            }
            None => lines.push("§7- §fMain TPS: §a20.00 §7(no sample yet)".to_string()),
        }

        if regions.is_empty() {
            lines.push("§7- §fNo region data yet".to_string());
            return lines;
        }

        let mut by_util = regions.to_vec();
        by_util.sort_by(|a, b| b.utilisation().total_cmp(&a.utilisation()));
        let mut tps: Vec<f32> = regions.iter().map(|r| r.tps).collect();
        tps.sort_by(f32::total_cmp);

        lines.push(format!(
            "§7- §fUtilisation: §a{:.1}%§7 / 100.0%",
            by_util[0].utilisation()
        ));
        lines.push(format!("§7- §fLowest Region TPS: §a{:.2}", tps[0]));
        lines.push(format!("§7- §fMedian Region TPS: §a{:.2}", tps[tps.len() / 2]));
        lines.push(format!("§7- §fHighest Region TPS: §a{:.2}", tps[tps.len() - 1]));
        lines.push("§bHighest 3 utilisation regions".to_string());

        for region in by_util.iter().take(3) {
            let cx = region.region.0 * REGION_BLOCKS + REGION_BLOCKS / 2;
            let cz = region.region.1 * REGION_BLOCKS + REGION_BLOCKS / 2;
            lines.push(format!("§7- §fRegion around block (w:'§b{world}§f',{cx},64,{cz})"));
            lines.push(format!(
                "  §a{:.1}%§f util at §a{:.2} MSPT§f at §a{:.2} TPS",
                region.utilisation(),
                region.mspt,
                region.tps
            ));
            lines.push(format!(
                "  §fChunks: §a{}§f Players: §a{}§f Entities: §a{}§f Peak: §a{:.2} ms",
                region.chunks, region.players, region.entities, region.max_mspt
            ));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(tps: f64, avg: f64, max: f64, lag: f64) -> TickStats {
        TickStats {
            tps,
            mspt_avg: avg,
            mspt_max: max,
            lag_ms: lag,
            samples: 1200,
            ticks: 88_888,
        }
    }

    fn region(x: i32, z: i32, mspt: f32, tps: f32) -> RegionHealth {
        RegionHealth {
            region: (x, z),
            mspt,
            max_mspt: mspt * 2.0,
            tps,
            chunks: 650,
            players: 1,
            entities: 252,
        }
    }

    #[test]
    fn reports_tps_and_mspt_lines() {
        let regions = [region(0, 0, 7.25, 20.0)];
        let lines = CommandTps::Tps.execute(Some(stats(19.5, 1.234, 12.0, 480.0)), &regions, 1, "world");
        assert!(lines[3].contains("§a19.50 TPS"), "容差内不该标红: {}", lines[3]);
        let lines = CommandTps::Tps.execute(Some(stats(19.0, 1.234, 12.0, 480.0)), &regions, 1, "world");
        assert_eq!(lines[0], "§bServer Health Report");
        assert_eq!(lines[1], "§7- §fOnline Players: §a1");
        assert_eq!(lines[2], "§7- §fTotal regions: §a1");
        assert!(lines[3].contains("§c19.00 TPS"), "主循环掉帧要标红: {}", lines[3]);
        assert_eq!(lines[4], "§7- §fUtilisation: §a14.5%§7 / 100.0%");
        assert_eq!(lines[5], "§7- §fLowest Region TPS: §a20.00");
        assert!(lines.iter().any(|line| line.contains("§a14.5%§f util at §a7.25 MSPT")));
        assert!(lines.iter().any(|line| line.contains("Chunks: §a650")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("(w:'§bworld§f',256,64,256)")),
            "分片坐标换算不对: {:?}",
            lines.last()
        );
    }

    #[test]
    fn sorts_regions_by_utilisation_and_keeps_three() {
        let regions = [
            region(0, 0, 1.0, 20.0),
            region(1, 0, 20.0, 12.0),
            region(0, 1, 10.0, 20.0),
            region(1, 1, 40.0, 5.0),
        ];
        let lines = CommandTps::Tps.execute(None, &regions, 4, "world");
        assert!(lines[4].contains("80.0%"), "最高占用要排第一: {}", lines[4]);
        assert_eq!(
            lines.iter().filter(|line| line.contains("Region around block")).count(),
            3
        );
        assert!(!lines.iter().any(|line| line.contains("§a1.0%§f util")));
    }

    #[test]
    fn no_sample_yet_reports_nominal_tps() {
        let lines = CommandTps::Tps.execute(None, &[], 0, "world");
        assert_eq!(lines[1], "§7- §fOnline Players: §a0");
        assert!(lines[3].contains("no sample yet"));
        assert!(lines[4].contains("No region data yet"));
    }
}
