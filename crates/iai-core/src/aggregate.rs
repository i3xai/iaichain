//! 结果聚合：把各角色子任务的产出合并为一份交付（`Executed → Aggregated`）。

/// 把 `(role, content)` 列表聚合为分节文本。
pub fn aggregate(parts: &[(String, String)]) -> String {
    parts
        .iter()
        .map(|(role, content)| format!("## {role}\n{content}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_sections() {
        let out = aggregate(&[
            ("后端".into(), "实现 A".into()),
            ("测试".into(), "覆盖 B".into()),
        ]);
        assert!(out.contains("## 后端"));
        assert!(out.contains("## 测试"));
        assert!(out.contains("实现 A"));
    }
}
