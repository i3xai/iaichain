//! 贡献点计量（FR-011）。
//!
//! 钱包视图完全由账本推导 —— 账本是唯一事实源，余额不单独存储，避免与链不一致。

use crate::ledger::{LedgerEntry, LedgerKind};
use serde::Serialize;

const WEEK_SECS: i64 = 7 * 24 * 3600;

/// 钱包视图（字段与前端 `getWallet` 契约一致）。
#[derive(Debug, Clone, Serialize)]
pub struct WalletView {
    /// 可用余额 = Σ amount。
    pub balance: i64,
    /// 任务中锁定 = Σ locked_delta。
    pub locked: i64,
    /// 本周收益 = 近 7 天 settle/reward 的正向 amount 之和。
    pub weekly: i64,
    /// 当前净锁定笔数（lock 笔数 − unlock 笔数）。
    pub locked_tasks: i64,
    /// 近 7 天被采纳（settle/reward）的笔数。
    pub weekly_accepted: i64,
}

/// 由账本条目推导钱包视图。`now_epoch` 用于「本周」窗口判定。
pub fn derive_wallet(entries: &[LedgerEntry], now_epoch: i64) -> WalletView {
    let week_start = now_epoch - WEEK_SECS;
    let mut w = WalletView {
        balance: 0,
        locked: 0,
        weekly: 0,
        locked_tasks: 0,
        weekly_accepted: 0,
    };
    for e in entries {
        w.balance += e.amount;
        w.locked += e.locked_delta;
        match e.kind {
            LedgerKind::Lock => w.locked_tasks += 1,
            LedgerKind::Unlock => w.locked_tasks -= 1,
            LedgerKind::Settle | LedgerKind::Reward => {
                if e.ts_epoch >= week_start {
                    w.weekly_accepted += 1;
                    if e.amount > 0 {
                        w.weekly += e.amount;
                    }
                }
            }
            _ => {}
        }
    }
    if w.locked_tasks < 0 {
        w.locked_tasks = 0;
    }
    w
}
