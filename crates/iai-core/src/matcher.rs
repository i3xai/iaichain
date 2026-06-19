//! 子任务到节点的匹配（FR-004 / FR-006）。
//!
//! 仅依据团队成员的角色与在线状态匹配，按累计贡献评分择优；角色无在线匹配时
//! 回退到任一在线成员（队长兜底执行），保证不因单角色缺位而阻塞（FR-009 思路）。

use iai_node::registry::TeamMember;

/// 为某角色子任务选择执行节点，返回 node_id。
pub fn match_node(role: &str, members: &[TeamMember]) -> Option<String> {
    // 1) 角色精确匹配 + 在线，贡献高者优先
    let mut cands: Vec<&TeamMember> =
        members.iter().filter(|m| m.online && m.role == role).collect();
    cands.sort_by(|a, b| b.credits.cmp(&a.credits));
    if let Some(m) = cands.first() {
        return Some(m.node_id.clone());
    }
    // 2) 兜底：任一在线成员（优先非本机，其次本机队长）
    members
        .iter()
        .find(|m| m.online && !m.is_self)
        .or_else(|| members.iter().find(|m| m.online))
        .map(|m| m.node_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(node: &str, role: &str, online: bool, credits: i64, is_self: bool) -> TeamMember {
        TeamMember {
            node_id: node.into(),
            role: role.into(),
            model: "x".into(),
            online,
            credits,
            is_self,
        }
    }

    #[test]
    fn picks_highest_credit_matching_role() {
        let team = vec![
            m("a", "后端", true, 100, false),
            m("b", "后端", true, 900, false),
            m("c", "测试", true, 500, false),
        ];
        assert_eq!(match_node("后端", &team), Some("b".into()));
    }

    #[test]
    fn falls_back_to_any_online_non_self() {
        let team = vec![
            m("self", "队长", true, 0, true),
            m("d", "文档", true, 10, false),
        ];
        // 无「审查」角色 → 回退非本机在线成员 d
        assert_eq!(match_node("审查", &team), Some("d".into()));
    }

    #[test]
    fn falls_back_to_self_when_alone() {
        let team = vec![m("self", "队长", true, 0, true)];
        assert_eq!(match_node("后端", &team), Some("self".into()));
    }
}
