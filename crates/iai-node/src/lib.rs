//! 节点运行层（IAI Node Layer）。
//!
//! 阶段 1：本机节点身份与模型配置的领域类型。持久化（含 Provider key）由应用层
//! `iai-cli/storage` 负责；P2P 发现、Provider 调用适配在后续阶段（4 / 5）落地。
//! 字段对齐 `specs/001-task-orchestration/contracts/node-contract.md` 与 data-model。

use serde::{Deserialize, Serialize};

/// crate 版本（与 workspace 对齐）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 节点状态（data-model：Available / Busy / Offline）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Available,
    Busy,
    Offline,
}

impl NodeStatus {
    /// 是否对外在线（非 Offline 即视为在线）。
    pub fn is_online(self) -> bool {
        !matches!(self, NodeStatus::Offline)
    }
}

/// 节点角色。设计中本机默认作为「队长」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Captain,
    Member,
}

impl NodeRole {
    /// 中文展示名（前端 topbar / 概览卡使用）。
    pub fn display_zh(self) -> &'static str {
        match self {
            NodeRole::Captain => "队长",
            NodeRole::Member => "队员",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NodeRole::Captain => "captain",
            NodeRole::Member => "member",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "member" => NodeRole::Member,
            _ => NodeRole::Captain,
        }
    }
}

/// 大模型 Provider。核心层不感知其内部实现（章程 I）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAI,
    Anthropic,
    Ollama,
    Other(String),
}

impl Provider {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "openai" | "gpt" => Provider::OpenAI,
            "anthropic" | "claude" => Provider::Anthropic,
            "ollama" | "local" => Provider::Ollama,
            other => Provider::Other(other.to_string()),
        }
    }

    /// 存储用的小写标识。
    pub fn id(&self) -> String {
        match self {
            Provider::OpenAI => "openai".into(),
            Provider::Anthropic => "anthropic".into(),
            Provider::Ollama => "ollama".into(),
            Provider::Other(s) => s.clone(),
        }
    }

    /// 前端展示前缀。
    pub fn display(&self) -> String {
        match self {
            Provider::OpenAI => "OpenAI".into(),
            Provider::Anthropic => "Anthropic".into(),
            Provider::Ollama => "本地 Ollama".into(),
            Provider::Other(s) => s.clone(),
        }
    }

    /// 未显式指定 --model 时的默认模型名。
    pub fn default_model(&self) -> &str {
        match self {
            Provider::OpenAI => "gpt-4o",
            Provider::Anthropic => "claude-3-5-sonnet",
            Provider::Ollama => "qwen",
            Provider::Other(_) => "default",
        }
    }

    /// 本地 Provider 无需 API key。
    pub fn requires_key(&self) -> bool {
        !matches!(self, Provider::Ollama)
    }
}

/// 生成节点 id：`<role 前缀>.<6 位十六进制>`，全局唯一性由注册时校验保证（FR-005）。
/// 阶段 1 用进程信息 + 单调时间派生熵，避免额外依赖；后续可换更强随机源。
pub fn gen_node_id(role: NodeRole) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mix = nanos ^ ((std::process::id() as u128) << 17).wrapping_mul(0x9E3779B97F4A7C15);
    let prefix = match role {
        NodeRole::Captain => "captain",
        NodeRole::Member => "node",
    };
    format!("{prefix}.{:06x}", (mix as u32) & 0x00FF_FFFF)
}

/// 根据已配置模型数推导能力声明。
///
/// 设计立场：「配置大模型才是具备 AI 能力的节点」。阶段 1 先以「是否配置模型」
/// 区分能力有无；细粒度能力（reasoning/coding/...）在任务编排阶段（5）细化。
pub fn derive_capabilities(model_count: usize) -> Vec<String> {
    if model_count == 0 {
        vec![]
    } else {
        vec!["reasoning".into(), "coding".into(), "writing".into()]
    }
}
