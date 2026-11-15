//! 打印 getHeights 用到的四路噪声原始值：noise_dump <seed> <cx> <cz>
//! 依次输出 depth2d(25) main(425) n20(425) n24(425)，每段一行
use mojor::level::generator::mcpe_noise::NoiseStack;

fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let st = NoiseStack::new(a[0] as u32);
    let pos = (a[1] as f64 * 4.0, 0.0, a[2] as f64 * 4.0);
    let p = |v: Vec<f64>| println!("{}", v.iter().map(|x| format!("{x:.4}")).collect::<Vec<_>>().join(" "));
    p(st.depth_noise.get_region(pos, 5, 1, 5, (200.0, 200.0, 0.5)));
    p(st.main_noise.get_region(pos, 5, 17, 5, (684.412 / 80.0, 684.412 / 160.0, 684.412 / 80.0)));
    p(st.min_limit.get_region(pos, 5, 17, 5, (684.412, 855.515, 684.412)));
    p(st.max_limit.get_region(pos, 5, 17, 5, (684.412, 684.412, 684.412)));
}
