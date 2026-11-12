// 服务端世界时间 照搬 Nukkit Level 的 checkTime / sendTime / setTime
// Nukkit 参考: level/Level.java:567-614、2546-2571
use bytes::Bytes;

use crate::network::protocol::set_time::SetTimePacket;

// 照搬 Level 里 ++sendTimeTicker == 200 时向全体同步一次时间
const SEND_TIME_TICKS: u32 = 200;

#[derive(Debug, Clone)]
pub(crate) struct WorldTime {
    time: i32,
    stop_time: bool,
    send_ticker: u32,
}

impl Default for WorldTime {
    fn default() -> Self {
        // 与改造前 login_sequence 固定下发的 6000(正午) 保持一致
        Self {
            time: 6000,
            stop_time: false,
            send_ticker: 0,
        }
    }
}

impl WorldTime {
    pub(crate) fn time(&self) -> i32 {
        self.time
    }

    // 照搬 checkTime: 未停时每 tick +1
    // 返回 true 表示到了 200 tick 同步窗口
    pub(crate) fn tick(&mut self) -> bool {
        if !self.stop_time {
            self.time += 1;
        }
        self.send_ticker += 1;
        if self.send_ticker >= SEND_TIME_TICKS {
            self.send_ticker = 0;
            true
        } else {
            false
        }
    }

    // 照搬 sendTime: stopTime 时先补一包 started=true 再发 started=false
    // 登录同步与周期广播都用这一组包
    pub(crate) fn time_packets(&self) -> Vec<Bytes> {
        let mut packets = Vec::with_capacity(2);
        if self.stop_time {
            packets.push(
                SetTimePacket {
                    time: self.time,
                    started: true,
                }
                .encode(),
            );
        }
        packets.push(
            SetTimePacket {
                time: self.time,
                started: !self.stop_time,
            }
            .encode(),
        );
        packets
    }

    pub(crate) fn set_time(&mut self, value: i32) {
        self.time = value;
    }

    pub(crate) fn start_time(&mut self) {
        self.stop_time = false;
    }

    pub(crate) fn stop_time(&mut self) {
        self.stop_time = true;
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_one_tick_and_syncs_every_200() {
        let mut time = WorldTime::default();
        let initial = time.time();
        for _ in 0..199 {
            assert!(!time.tick());
        }
        assert!(time.tick(), "第 200 tick 要同步");
        assert_eq!(time.time(), initial + 200);
    }

    #[test]
    fn stopped_time_does_not_advance() {
        let mut time = WorldTime::default();
        time.stop_time();
        let before = time.time();
        for _ in 0..250 {
            time.tick();
        }
        assert_eq!(time.time(), before);
        assert!(time.stop_time);
        assert_eq!(time.time_packets().len(), 2, "停时要发两包");
    }

    #[test]
    fn running_time_sends_one_packet() {
        let time = WorldTime::default();
        assert_eq!(time.time_packets().len(), 1);
    }
}
