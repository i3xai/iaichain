//! 核心调度层（IAI Core Layer）。
//!
//! 阶段 5：任务七态生命周期（[`lifecycle`]）、提示词分解（[`decompose`]）、
//! 角色匹配（[`matcher`]）、质量门禁（[`quality`]）、结果聚合（[`aggregate`]），
//! 以及节点执行契约 [`provider`]（含确定性 [`provider::MockProvider`]）。
//!
//! 真实 LLM Provider 的 HTTP 适配、结算记账分发与 SSE 实时推送在阶段 6 落地。

pub mod aggregate;
pub mod decompose;
pub mod lifecycle;
pub mod matcher;
pub mod provider;
pub mod quality;

/// crate 版本（与 workspace 对齐）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 生成任务 id：`task.<8 位十六进制>`。阶段 1 同款时间派生熵，避免额外依赖。
pub fn gen_task_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mix = nanos ^ ((std::process::id() as u128) << 21).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("task.{:08x}", (mix as u64) & 0xFFFF_FFFF)
}
