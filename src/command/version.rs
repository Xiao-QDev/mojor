use sysinfo::System;
use std::sync::LazyLock;

static BASE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "This server is running Mojor version {} (MC: {}|API: Lunar API)",
        env!("CARGO_PKG_VERSION"),
        crate::config::server::VERSION
    )
});

fn arch_name(raw: &str) -> &str {
    match raw {
        "x86_64" | "amd64" => "AMD64",
        "aarch64" | "arm64" => "ARM64",
        "x86" | "i686" | "i386" => "x86",
        "arm" | "armv7l" => "ARM",
        "riscv64" => "RISC-V64",
        other => other,
    }
}

static OS: LazyLock<String> = LazyLock::new(|| {
    let os = System::long_os_version()
        .unwrap_or_else(|_| std::env::consts::OS.to_string());
    let kernel = System::kernel_version()
        .unwrap_or_else(|_| "unknown".to_string());
    let binding = System::cpu_arch();
    let arch = arch_name(&binding);
    format!("§7Running on {os} (kernel: §7{kernel} §7| §7arch: §7{arch})§r")
});

#[derive(Eq, PartialEq, Debug)]
pub enum CommandVersion {
    Version,
}

impl CommandVersion {
    pub fn lines(&self) -> [&'static str; 2] {
        [BASE.as_str(), OS.as_str()]
    }
}