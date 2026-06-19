//! 提示词分解：把自然语言需求拆成按角色的子任务（FR-003）。
//!
//! 阶段 5 用确定性关键词规则（无 LLM 依赖，可复现）；阶段 6 可换为模型驱动分解。

/// 一个角色子任务规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleSpec {
    /// 角色展示名：后端 / 前端 / 测试 / 审查 / 文档。
    pub role: String,
}

/// 按关键词把需求分解为角色子任务。始终包含「后端 + 测试」兜底，至少 2 个角色。
pub fn decompose(prompt: &str) -> Vec<RoleSpec> {
    let p = prompt;
    let mut roles: Vec<String> = Vec::new();
    let mut push = |r: &str, roles: &mut Vec<String>| {
        if !roles.iter().any(|x| x == r) {
            roles.push(r.to_string());
        }
    };

    let has = |kws: &[&str]| kws.iter().any(|k| p.contains(k));

    // 后端：默认且高频
    push("后端", &mut roles);
    if has(&["前端", "UI", "ui", "界面", "页面", "组件"]) {
        push("前端", &mut roles);
    }
    // 测试：始终包含
    push("测试", &mut roles);
    if has(&["审查", "review", "安全", "审计"]) {
        push("审查", &mut roles);
    }
    if has(&["文档", "doc", "说明", "README", "readme"]) {
        push("文档", &mut roles);
    }

    roles.into_iter().map(|role| RoleSpec { role }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(p: &str) -> Vec<String> {
        decompose(p).into_iter().map(|r| r.role).collect()
    }

    #[test]
    fn defaults_to_backend_and_test() {
        assert_eq!(roles("实现一个限流中间件"), vec!["后端", "测试"]);
    }

    #[test]
    fn expands_by_keywords() {
        let r = roles("做一个带 UI 界面的 JWT 鉴权，附文档与安全审查");
        assert!(r.contains(&"前端".to_string()));
        assert!(r.contains(&"审查".to_string()));
        assert!(r.contains(&"文档".to_string()));
        assert!(r.contains(&"后端".to_string()));
        assert!(r.contains(&"测试".to_string()));
    }
}
