//! 节点运行层（IAI Node Layer）。
//!
//! 阶段 0：占位骨架。后续阶段在此落地节点身份/能力契约、注册表、P2P 发现与 Provider 适配。
//! 参见 `DEVELOPMENT-PLAN.md` 阶段 1 与阶段 4。

/// crate 版本（与 workspace 对齐）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
