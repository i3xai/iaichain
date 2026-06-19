//! 任务七态生命周期（FR-002）。单向推进，非法跳转被拒绝。

use serde::{Deserialize, Serialize};

/// 任务状态机：`Created → Parsed → Decomposed → Matched → Executed → Aggregated → Settled`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Created,
    Parsed,
    Decomposed,
    Matched,
    Executed,
    Aggregated,
    Settled,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Created => "created",
            TaskState::Parsed => "parsed",
            TaskState::Decomposed => "decomposed",
            TaskState::Matched => "matched",
            TaskState::Executed => "executed",
            TaskState::Aggregated => "aggregated",
            TaskState::Settled => "settled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "parsed" => TaskState::Parsed,
            "decomposed" => TaskState::Decomposed,
            "matched" => TaskState::Matched,
            "executed" => TaskState::Executed,
            "aggregated" => TaskState::Aggregated,
            "settled" => TaskState::Settled,
            _ => TaskState::Created,
        }
    }

    pub fn display_zh(self) -> &'static str {
        match self {
            TaskState::Created => "已创建",
            TaskState::Parsed => "已解析",
            TaskState::Decomposed => "已分解",
            TaskState::Matched => "已匹配",
            TaskState::Executed => "已执行",
            TaskState::Aggregated => "已采纳",
            TaskState::Settled => "已结算",
        }
    }

    /// 下一个合法状态（已到终态返回 None）。
    pub fn next(self) -> Option<TaskState> {
        match self {
            TaskState::Created => Some(TaskState::Parsed),
            TaskState::Parsed => Some(TaskState::Decomposed),
            TaskState::Decomposed => Some(TaskState::Matched),
            TaskState::Matched => Some(TaskState::Executed),
            TaskState::Executed => Some(TaskState::Aggregated),
            TaskState::Aggregated => Some(TaskState::Settled),
            TaskState::Settled => None,
        }
    }

    /// 仅允许相邻向前转移（FR-002）。
    pub fn can_transition(self, to: TaskState) -> bool {
        self.next() == Some(to)
    }

    /// 是否对用户视为「完成/已交付」（已采纳或已结算）。
    pub fn is_delivered(self) -> bool {
        matches!(self, TaskState::Aggregated | TaskState::Settled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_adjacent_forward_allowed() {
        assert!(TaskState::Created.can_transition(TaskState::Parsed));
        assert!(!TaskState::Created.can_transition(TaskState::Decomposed));
        assert!(!TaskState::Matched.can_transition(TaskState::Created));
        assert_eq!(TaskState::Settled.next(), None);
    }

    #[test]
    fn full_chain() {
        let mut s = TaskState::Created;
        let mut steps = 0;
        while let Some(n) = s.next() {
            assert!(s.can_transition(n));
            s = n;
            steps += 1;
        }
        assert_eq!(steps, 6);
        assert_eq!(s, TaskState::Settled);
    }
}
