//! 对应 `rtg/carver/MathHelper.php`。
//!
//! PHP 从 `sin-table.bin` 里读 65536 个 float（Java `MathHelper.SIN_TABLE`
//! 的序列化副本）。这里不做资源文件：同一公式现算，
//! `SIN_TABLE[i] = (float) sin(i * PI * 2.0 / 65536.0)`，
//! 已用脚本逐项核对 65536 项与二进制表完全一致。
//!
//! 精度要点：索引先按 `f()` 截断成 float32 再取整，和 PHP 的
//! `(int) F::f($value * 10430.3779296875)` 一致。

use crate::level::generator::realistic_terrain::math::f;

/// `65536 / (2π)` 的 float32 值，Java 里写成 `10430.378F`。
const SIN_MULTIPLIER: f64 = 10430.3779296875;

fn table(index: i32) -> f64 {
    let angle = (index & 65535) as f64 * std::f64::consts::PI * 2.0 / 65536.0;
    angle.sin() as f32 as f64
}

pub fn sin(value: f64) -> f64 {
    table(f(value * SIN_MULTIPLIER) as i32)
}

pub fn cos(value: f64) -> f64 {
    table(f(f(value * SIN_MULTIPLIER) + 16384.0) as i32)
}

/// `MathHelper::floor` = `(int) floor($value)`，向负无穷取整后再截断成 int。
pub fn floor(value: f64) -> i32 {
    value.floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turns() {
        assert_eq!(table(0), 0.0);
        assert_eq!(table(16384), 1.0);
        assert_eq!(table(49152), -1.0);
        // sin(π) 在浮点里不是精确 0
        assert!(table(32768).abs() < 1e-15);
        // cos 是 sin 右移 16384
        assert_eq!(cos(0.0), table(16384));
        assert_eq!(sin(std::f64::consts::FRAC_PI_2), table(16384));
    }

    #[test]
    fn index_wraps_on_negative() {
        let value = -0.000_1_f64;
        let index = f(value * SIN_MULTIPLIER) as i32;
        assert!(index < 0, "下标应为负，才能覆盖 & 65535 分支");
        assert_eq!(sin(value), table(index));
    }
}
