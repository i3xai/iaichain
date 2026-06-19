//! 节点执行契约（核心层 ↔ 节点层，章程 I / node-contract）。
//!
//! 核心层只通过 [`ModelProvider`] 与节点交互，不感知具体模型实现。
//! 阶段 5 提供确定性 [`MockProvider`]（无外部依赖，用于离线/无 key 的编排验证）；
//! 真实 OpenAI / Anthropic / Ollama 的 HTTP 适配在阶段 6 落地（同样实现本 trait）。

/// 子任务执行请求。
#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub subtask_id: String,
    pub role: String,
    pub prompt: String,
    pub repo: String,
}

/// 执行输出，必须携带来源节点以支撑追溯（FR-016）。
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub content: String,
    pub source_node_id: String,
}

/// 执行错误（超时 / Provider 错误 / 能力不匹配）。
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("执行超时")]
    Timeout,
    #[error("Provider 错误: {0}")]
    ProviderError(String),
    #[error("能力不匹配")]
    CapabilityMismatch,
}

/// 节点执行契约。
pub trait ModelProvider {
    fn execute(&self, req: &ExecRequest, node_id: &str) -> Result<ExecOutput, ExecError>;
}

/// 确定性 Mock Provider：模拟各角色产出，无外部依赖、结果可复现。
pub struct MockProvider;

impl ModelProvider for MockProvider {
    fn execute(&self, req: &ExecRequest, node_id: &str) -> Result<ExecOutput, ExecError> {
        let content = format!(
            "[{}] 针对「{}」(repo: {}) 的产出：已完成实现并自检，要点覆盖完整。",
            req.role, req.prompt, req.repo
        );
        Ok(ExecOutput {
            content,
            source_node_id: node_id.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_produces_traceable_output() {
        let p = MockProvider;
        let req = ExecRequest {
            subtask_id: "s1".into(),
            role: "后端".into(),
            prompt: "限流中间件".into(),
            repo: "github.com/acme/x".into(),
        };
        let out = p.execute(&req, "node.4a91").unwrap();
        assert_eq!(out.source_node_id, "node.4a91");
        assert!(out.content.contains("后端"));
        assert!(out.content.contains("限流中间件"));
    }
}
