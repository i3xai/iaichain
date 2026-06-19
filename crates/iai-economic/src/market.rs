//! 自由市场定价与撮合（FR-012 / SC-006）。
//!
//! 核心规则：买方只能**从最低价开始逐笔向上吃单**，价格完全由公开挂卖簿决定。
//! 本模块只提供纯撮合逻辑；挂卖簿持久化与成交落账由 `iai-cli/storage` 负责。
//!
//! 金额一律以「分」（cents，整数）存储与计算，避免浮点误差；对外展示再转「元」。

use serde::Serialize;

/// 一条挂卖单。
#[derive(Debug, Clone, Serialize)]
pub struct Ask {
    pub id: i64,
    pub px_cents: i64,
    pub qty: i64,
    pub node_id: String,
}

/// 买入计划：从最低价逐笔吃单的结果（纯函数，不改库）。
#[derive(Debug, Clone)]
pub struct BuyPlan {
    /// 逐笔吃单明细：(ask_id, 成交数量)。
    pub takes: Vec<(i64, i64)>,
    /// 实际成交数量（供给不足时 < need）。
    pub filled: i64,
    /// 成交总额（分）。
    pub cost_cents: i64,
}

/// 计算买入计划。`asks_sorted` 必须按 `px_cents` 升序。
/// `need <= 0` 或无挂单时返回空计划。
pub fn plan_buy(asks_sorted: &[Ask], need: i64) -> BuyPlan {
    let mut takes = Vec::new();
    let mut filled = 0;
    let mut cost_cents = 0;
    let mut remain = need.max(0);
    for a in asks_sorted {
        if remain <= 0 {
            break;
        }
        let take = remain.min(a.qty);
        if take <= 0 {
            continue;
        }
        takes.push((a.id, take));
        filled += take;
        cost_cents += take * a.px_cents;
        remain -= take;
    }
    BuyPlan { takes, filled, cost_cents }
}

/// 分 → 元。
pub fn yuan(px_cents: i64) -> f64 {
    px_cents as f64 / 100.0
}

/// 元 → 分（四舍五入）。
pub fn cents_from_yuan(y: f64) -> i64 {
    (y * 100.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(id: i64, px: i64, qty: i64) -> Ask {
        Ask { id, px_cents: px, qty, node_id: format!("n{id}") }
    }

    #[test]
    fn buys_from_lowest_upward() {
        // 86×80 + 88×40 = 6880 + 3520 = 10400 分，成交 120
        let asks = vec![ask(1, 86, 80), ask(2, 88, 150), ask(3, 91, 60)];
        let plan = plan_buy(&asks, 120);
        assert_eq!(plan.filled, 120);
        assert_eq!(plan.cost_cents, 80 * 86 + 40 * 88);
        assert_eq!(plan.takes, vec![(1, 80), (2, 40)]);
    }

    #[test]
    fn partial_fill_when_supply_short() {
        let asks = vec![ask(1, 86, 50)];
        let plan = plan_buy(&asks, 120);
        assert_eq!(plan.filled, 50);
        assert_eq!(plan.cost_cents, 50 * 86);
    }

    #[test]
    fn empty_book_or_zero_need() {
        assert_eq!(plan_buy(&[], 100).filled, 0);
        assert_eq!(plan_buy(&[ask(1, 86, 50)], 0).filled, 0);
    }
}
