//! Automatic model selection.
//!
//! All candidate models are open-weight releases with permissive licenses
//! that are safe for a commercial product used inside a company with fewer
//! than 100 employees (no royalty, no per-seat fee, no "source-available but
//! non-commercial" traps):
//!
//! - `bge-m3`      (BAAI)      - MIT license. Excellent multilingual /
//!                                Chinese + English embeddings.
//! - `qwen2.5:*`    (Alibaba)   - Apache License 2.0 for the 0.5B-14B and 32B
//!                                sizes used here. No usage caps. Strong
//!                                Chinese + English generation quality.
//! - `llama3.1:*`, `llama3.2:*` (Meta) - Meta Llama Community License: free
//!                                for commercial use for organizations with
//!                                under 700M monthly active users (far above
//!                                a <100-person company), attribution
//!                                ("Built with Llama") required in the About
//!                                screen - see AboutDialog.tsx.

use crate::hardware::{capability_tier, CapabilityTier, HardwareInfo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// The exact tag passed to `ollama pull` / `ollama run`.
    pub tag: String,
    pub display_name: String,
    pub license: String,
    pub approx_download_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChoice {
    pub embedding: ModelSpec,
    pub chat: ModelSpec,
    /// Vector dimension produced by the chosen embedding model, needed when
    /// creating the LanceDB table schema.
    pub embedding_dim: i32,
    pub tier: String,
    pub reason: String,
}

fn bge_m3() -> ModelSpec {
    ModelSpec {
        tag: "bge-m3".to_string(),
        display_name: "BAAI bge-m3 (向量模型)".to_string(),
        license: "MIT".to_string(),
        approx_download_gb: 1.2,
    }
}

fn qwen(tag: &str, display: &str, gb: f64) -> ModelSpec {
    ModelSpec {
        tag: tag.to_string(),
        display_name: display.to_string(),
        license: "Apache-2.0".to_string(),
        approx_download_gb: gb,
    }
}

fn llama(tag: &str, display: &str, gb: f64) -> ModelSpec {
    ModelSpec {
        tag: tag.to_string(),
        display_name: display.to_string(),
        license: "Llama 3 Community License (免费商用，<7亿月活用户)".to_string(),
        approx_download_gb: gb,
    }
}

pub fn select_models(hw: &HardwareInfo) -> ModelChoice {
    let tier = capability_tier(hw);
    let embedding = bge_m3();
    let embedding_dim = 1024;

    let (chat, tier_label) = match (tier, hw.is_chinese_locale) {
        (CapabilityTier::Low, true) => (
            qwen("qwen2.5:1.5b", "Qwen2.5 1.5B Instruct", 1.0),
            "low",
        ),
        (CapabilityTier::Low, false) => (
            llama("llama3.2:3b", "Llama 3.2 3B Instruct", 2.0),
            "low",
        ),
        (CapabilityTier::Mid, true) => (
            qwen("qwen2.5:7b", "Qwen2.5 7B Instruct", 4.7),
            "mid",
        ),
        (CapabilityTier::Mid, false) => (
            llama("llama3.1:8b", "Llama 3.1 8B Instruct", 4.7),
            "mid",
        ),
        (CapabilityTier::High, true) => (
            qwen("qwen2.5:14b", "Qwen2.5 14B Instruct", 9.0),
            "high",
        ),
        (CapabilityTier::High, false) => (
            llama("llama3.1:8b", "Llama 3.1 8B Instruct", 4.7),
            "high",
        ),
    };

    let reason = format!(
        "检测到内存约 {:.0}GB，{}GPU，系统语言{}为中文 → 选择 {} 档模型",
        hw.total_memory_gb,
        if hw.has_capable_gpu { "具备可用" } else { "未检测到独立" },
        if hw.is_chinese_locale { "" } else { "不" },
        tier_label
    );

    ModelChoice {
        embedding,
        chat,
        embedding_dim,
        tier: tier_label.to_string(),
        reason,
    }
}
