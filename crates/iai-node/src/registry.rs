//! 节点注册表与团队成员（FR-005 / FR-006）。
//!
//! 阶段 4：把团队成员节点登记到本地注册表，作为「去中心化服务网络」中本机已知的视图。
//! 节点身份/能力契约见 node-contract。
//!
//! 范围说明：自动发现的**传输层**（mDNS / DHT / tracker 跨网穿透）列为后续特性；
//! 当前注册表通过显式招募 / 邀请填充，是网络视图的事实源。

use serde::Serialize;

/// 团队成员节点（注册表持久化形态）。
#[derive(Debug, Clone, Serialize)]
pub struct TeamMember {
    pub node_id: String,
    /// 角色展示名：队长 / 前端 / 后端 / 测试 / 审查 / 文档 …
    pub role: String,
    /// 该成员配置的模型展示名。
    pub model: String,
    pub online: bool,
    /// 累计贡献点（去中心化下由成员自报 / 网络同步）。
    pub credits: i64,
    /// 是否本机节点。
    pub is_self: bool,
}

/// 网络概况（前端概览「网络」卡）。
#[derive(Debug, Clone, Serialize)]
pub struct NetworkStat {
    /// 在线成员数。
    pub members_online: i64,
    /// 已知（注册表内）节点数。
    pub discovered: i64,
    /// 已知公开团队数。
    pub public_teams: i64,
}

/// 贡献点千分位格式化（前端「累计贡献」列）。
pub fn format_credits(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let bytes = digits.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::format_credits;

    #[test]
    fn thousands_separator() {
        assert_eq!(format_credits(0), "0");
        assert_eq!(format_credits(640), "640");
        assert_eq!(format_credits(2180), "2,180");
        assert_eq!(format_credits(1234567), "1,234,567");
    }
}
