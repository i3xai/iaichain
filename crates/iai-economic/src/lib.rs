//! 经济系统层（Economic Layer）。
//!
//! 阶段 2：哈希链 append-only 账本（[`ledger`]）+ 由账本推导的钱包视图（[`credit`]）。
//! 阶段 3：自由市场撮合与定价（[`market`]）。

pub mod credit;
pub mod ledger;
pub mod market;

/// crate 版本（与 workspace 对齐）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
