//! 质量门禁（FR-008）。gate_passed=false 的结果不得进入聚合/结算（SC-007）。

use serde::Serialize;

/// 裁判阈值：judge_score ≥ 此值且规则通过，方可入聚合。
pub const THRESHOLD: f32 = 0.6;

/// 质量裁定结果。
#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub rules_passed: bool,
    pub judge_score: f32,
    pub gate_passed: bool,
}

/// 对执行结果做规则校验 + 裁判评分。
///
/// 阶段 5 用确定性伪评分（无 LLM 裁判依赖）：非空且长度达标即规则通过，
/// 评分由内容规模派生并钳制在 [0, 0.99]。阶段 6 可接入真实裁判模型。
pub fn quality_gate(content: &str) -> Verdict {
    let trimmed = content.trim();
    let rules_passed = !trimmed.is_empty() && trimmed.chars().count() >= 8;
    let judge_score = if rules_passed {
        (0.7 + (content.len() % 30) as f32 / 100.0).min(0.99)
    } else {
        0.0
    };
    Verdict {
        rules_passed,
        judge_score,
        gate_passed: rules_passed && judge_score >= THRESHOLD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_substantial_content() {
        let v = quality_gate("[后端] 针对「限流」的产出：实现完成");
        assert!(v.rules_passed && v.gate_passed && v.judge_score >= THRESHOLD);
    }

    #[test]
    fn rejects_empty_or_tiny() {
        assert!(!quality_gate("").gate_passed);
        assert!(!quality_gate("  短  ").gate_passed);
    }
}
