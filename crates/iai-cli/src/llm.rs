//! 真实 LLM Provider HTTP 适配（阶段 11）。
//!
//! 支持 MiniMax / OpenAI（兼容）/ Anthropic / Ollama。返回生成内容 + token 用量。
//! 节点执行时若配了真实 Provider（有 key 或本地 Ollama）→ 真实调用；否则回退 Mock。
//! key 仅在内存与请求头中使用，不落日志。

use anyhow::Context;
use serde_json::{json, Value};

/// LLM 调用配置（来自本机模型配置）。
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub key: Option<String>,
}

/// LLM 输出：内容 + token 用量。
pub struct LlmOutput {
    pub content: String,
    pub tokens: i64,
}

/// 是否为可真实调用的 Provider（有 key，或本地 Ollama 无需 key）。
pub fn is_real(cfg: &LlmConfig) -> bool {
    match cfg.provider.as_str() {
        "ollama" => true,
        "minimax" | "openai" | "anthropic" => cfg.key.as_deref().map(|k| !k.trim().is_empty()).unwrap_or(false),
        _ => false,
    }
}

/// 调用真实 LLM。endpoint 可由 `IAI_LLM_ENDPOINT` / `IAI_OLLAMA` 覆盖。
pub async fn call_llm(cfg: &LlmConfig, prompt: &str) -> anyhow::Result<LlmOutput> {
    match cfg.provider.as_str() {
        "ollama" => call_ollama(cfg, prompt).await,
        "minimax" => {
            let ep = endpoint_override().unwrap_or_else(|| "https://api.minimax.chat/v1/text/chatcompletion_v2".to_string());
            call_openai_compat(cfg, &ep, prompt).await
        }
        "openai" => {
            let ep = endpoint_override().unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
            call_openai_compat(cfg, &ep, prompt).await
        }
        "anthropic" => call_anthropic(cfg, prompt).await,
        other => anyhow::bail!("不支持的 Provider: {other}"),
    }
}

fn endpoint_override() -> Option<String> {
    std::env::var("IAI_LLM_ENDPOINT").ok().filter(|s| !s.trim().is_empty())
}

/// OpenAI 兼容 chat completions（MiniMax / OpenAI）。
async fn call_openai_compat(cfg: &LlmConfig, endpoint: &str, prompt: &str) -> anyhow::Result<LlmOutput> {
    let key = cfg.key.as_deref().filter(|k| !k.trim().is_empty()).context("缺少 API key")?;
    let body = json!({
        "model": cfg.model,
        "messages": [{ "role": "user", "content": prompt }]
    });
    let resp: Value = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let content = resp["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    let tokens = resp["usage"]["total_tokens"].as_i64().unwrap_or(0);
    if content.trim().is_empty() {
        anyhow::bail!("LLM 返回空内容: {}", resp.get("base_resp").cloned().unwrap_or(Value::Null));
    }
    Ok(LlmOutput { content, tokens })
}

/// Anthropic messages API。
async fn call_anthropic(cfg: &LlmConfig, prompt: &str) -> anyhow::Result<LlmOutput> {
    let key = cfg.key.as_deref().filter(|k| !k.trim().is_empty()).context("缺少 API key")?;
    let body = json!({
        "model": cfg.model,
        "max_tokens": 1024,
        "messages": [{ "role": "user", "content": prompt }]
    });
    let resp: Value = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let content = resp["content"][0]["text"].as_str().unwrap_or("").to_string();
    let tokens = resp["usage"]["input_tokens"].as_i64().unwrap_or(0)
        + resp["usage"]["output_tokens"].as_i64().unwrap_or(0);
    Ok(LlmOutput { content, tokens })
}

/// 本地 Ollama generate API。
async fn call_ollama(cfg: &LlmConfig, prompt: &str) -> anyhow::Result<LlmOutput> {
    let base = endpoint_override()
        .or_else(|| std::env::var("IAI_OLLAMA").ok())
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let body = json!({ "model": cfg.model, "prompt": prompt, "stream": false });
    let resp: Value = reqwest::Client::new()
        .post(format!("{base}/api/generate"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let content = resp["response"].as_str().unwrap_or("").to_string();
    let tokens = resp["eval_count"].as_i64().unwrap_or(0) + resp["prompt_eval_count"].as_i64().unwrap_or(0);
    Ok(LlmOutput { content, tokens })
}
