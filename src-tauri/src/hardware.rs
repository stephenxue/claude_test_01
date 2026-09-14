//! Best-effort detection of local hardware capability and locale, used to pick
//! a sensible default embedding model + chat model on first run.

use serde::{Deserialize, Serialize};
use std::process::Command;
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub total_memory_gb: f64,
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub is_apple_silicon: bool,
    /// True when we could positively confirm a usable GPU (Apple Silicon's
    /// integrated GPU always counts; on Intel Macs this is best-effort).
    pub has_capable_gpu: bool,
    pub is_chinese_locale: bool,
}

/// Coarse capability tier used to pick model sizes. Kept intentionally simple
/// (RAM-driven) because on macOS the GPU and CPU share unified memory on
/// Apple Silicon, which is the dominant case we optimize for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityTier {
    Low,
    Mid,
    High,
}

pub fn detect_hardware() -> HardwareInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_memory_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let cpu_brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let cpu_cores = sys.cpus().len().max(1);

    let is_apple_silicon = detect_apple_silicon();
    let has_capable_gpu = is_apple_silicon || detect_intel_discrete_gpu();
    let is_chinese_locale = detect_chinese_locale();

    HardwareInfo {
        total_memory_gb,
        cpu_brand,
        cpu_cores,
        is_apple_silicon,
        has_capable_gpu,
        is_chinese_locale,
    }
}

#[cfg(target_os = "macos")]
fn detect_apple_silicon() -> bool {
    std::env::consts::ARCH == "aarch64"
}

#[cfg(not(target_os = "macos"))]
fn detect_apple_silicon() -> bool {
    false
}

/// Best-effort: ask `system_profiler` whether a discrete GPU is present.
/// Not fatal if it fails or times out - we just fall back to "no GPU".
fn detect_intel_discrete_gpu() -> bool {
    let output = Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output();

    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            // Heuristic keywords used by AMD/NVIDIA discrete GPUs on older Intel Macs.
            text.contains("amd radeon") || text.contains("nvidia") || text.contains("vram")
        }
        Err(_) => false,
    }
}

/// Best-effort Chinese-locale detection: checks the macOS global locale
/// preference first, then falls back to the `LANG`/`LC_ALL` env vars.
fn detect_chinese_locale() -> bool {
    if let Ok(out) = Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        let locale = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
        if !locale.is_empty() {
            return locale.starts_with("zh");
        }
    }

    for key in ["LC_ALL", "LANG", "LANGUAGE"] {
        if let Ok(val) = std::env::var(key) {
            if val.to_lowercase().starts_with("zh") {
                return true;
            }
        }
    }
    false
}

pub fn capability_tier(hw: &HardwareInfo) -> CapabilityTier {
    if hw.total_memory_gb >= 32.0 && hw.has_capable_gpu {
        CapabilityTier::High
    } else if hw.total_memory_gb >= 16.0 {
        CapabilityTier::Mid
    } else {
        CapabilityTier::Low
    }
}
