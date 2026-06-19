//! 哈希链 append-only 账本（满足章程 IV / FR-010 / FR-013）。
//!
//! 本模块只提供**纯逻辑**：条目类型、规范化哈希、链校验。持久化（SQL 事务、
//! seq 分配、prev_hash 串接）由应用层 `iai-cli/storage` 负责，并复用此处的
//! [`compute_entry_hash`]，保证「写入」与「重算校验」用同一套规范化规则。

use chrono::{FixedOffset, TimeZone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 链起点的前序哈希（首条记录使用）。
pub const GENESIS_PREV: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// UTC+8（北京时间）时区偏移。全局时间口径见 CLAUDE.md。
fn utc8() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("UTC+8 偏移合法")
}

/// 账本条目类型。`amount` 影响可用余额，`locked_delta` 影响锁定池。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerKind {
    /// 任务结算收益（贡献被采纳）。
    Settle,
    /// 奖励分发。
    Reward,
    /// 任务发起锁定（可用 → 锁定）。
    Lock,
    /// 任务结束释放（锁定 → 可用）。
    Unlock,
    /// 市场买入贡献币。
    Buy,
    /// 市场卖出贡献币。
    Sell,
}

impl LedgerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LedgerKind::Settle => "settle",
            LedgerKind::Reward => "reward",
            LedgerKind::Lock => "lock",
            LedgerKind::Unlock => "unlock",
            LedgerKind::Buy => "buy",
            LedgerKind::Sell => "sell",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "settle" | "结算" => LedgerKind::Settle,
            "reward" | "奖励" => LedgerKind::Reward,
            "lock" | "锁定" => LedgerKind::Lock,
            "unlock" | "释放" => LedgerKind::Unlock,
            "buy" | "买入" => LedgerKind::Buy,
            "sell" | "卖出" => LedgerKind::Sell,
            _ => return None,
        })
    }

    /// 前端流水「类型」列展示名。
    pub fn display_zh(self) -> &'static str {
        match self {
            LedgerKind::Settle => "结算",
            LedgerKind::Reward => "奖励",
            LedgerKind::Lock => "锁定",
            LedgerKind::Unlock => "释放",
            LedgerKind::Buy => "买入",
            LedgerKind::Sell => "卖出",
        }
    }
}

/// 单条账本记录（持久化形态）。
#[derive(Debug, Clone, Serialize)]
pub struct LedgerEntry {
    pub seq: u64,
    pub ts_epoch: i64,
    pub kind: LedgerKind,
    pub node_id: String,
    pub amount: i64,
    pub locked_delta: i64,
    pub note: String,
    pub prev_hash: String,
    pub entry_hash: String,
}

/// 规范化字段并计算 entry_hash = sha256(seq|ts|kind|node|amount|locked|note|prev)。
///
/// 「写入」与「校验」必须调用此函数，确保哈希口径一致（FR-013）。
pub fn compute_entry_hash(
    seq: u64,
    ts_epoch: i64,
    kind: LedgerKind,
    node_id: &str,
    amount: i64,
    locked_delta: i64,
    note: &str,
    prev_hash: &str,
) -> String {
    let canon = format!(
        "{seq}|{ts_epoch}|{}|{node_id}|{amount}|{locked_delta}|{note}|{prev_hash}",
        kind.as_str()
    );
    let digest = Sha256::digest(canon.as_bytes());
    hex(&digest)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// 把 epoch 秒格式化为 UTC+8 的 `MM-DD HH:MM`（前端流水「时间」列）。
pub fn display_time(ts_epoch: i64) -> String {
    match utc8().timestamp_opt(ts_epoch, 0).single() {
        Some(dt) => dt.format("%m-%d %H:%M").to_string(),
        None => ts_epoch.to_string(),
    }
}

/// 校验失败原因（FR-013 / SC-004）。
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("第 {seq} 条 seq 不连续（期望 {expected}）")]
    SeqGap { seq: u64, expected: u64 },
    #[error("第 {seq} 条 prev_hash 与上一条不匹配")]
    PrevMismatch { seq: u64 },
    #[error("第 {seq} 条 entry_hash 重算不一致（疑似被篡改）")]
    HashMismatch { seq: u64 },
}

/// 重算整条链：seq 连续、prev_hash 串接、entry_hash 可复算一致。
///
/// `entries` 需按 seq 升序。空链视为有效。
pub fn verify_chain(entries: &[LedgerEntry]) -> Result<(), VerifyError> {
    let mut prev = GENESIS_PREV.to_string();
    for (idx, e) in entries.iter().enumerate() {
        let expected_seq = idx as u64 + 1;
        if e.seq != expected_seq {
            return Err(VerifyError::SeqGap { seq: e.seq, expected: expected_seq });
        }
        if e.prev_hash != prev {
            return Err(VerifyError::PrevMismatch { seq: e.seq });
        }
        let recomputed = compute_entry_hash(
            e.seq, e.ts_epoch, e.kind, &e.node_id, e.amount, e.locked_delta, &e.note, &e.prev_hash,
        );
        if recomputed != e.entry_hash {
            return Err(VerifyError::HashMismatch { seq: e.seq });
        }
        prev = e.entry_hash.clone();
    }
    Ok(())
}
