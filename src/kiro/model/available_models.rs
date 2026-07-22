//! ListAvailableModels 上游响应类型与合并逻辑

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 上游模型信息（ListAvailableModels）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamModelInfo {
    pub model_id: String,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "supportedInputTypes")]
    pub input_types: Vec<String>,
    #[serde(default)]
    pub rate_multiplier: Option<f64>,
    #[serde(default)]
    pub token_limits: Option<TokenLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TokenLimits {
    #[serde(default)]
    pub max_input_tokens: Option<i32>,
    #[serde(default)]
    pub max_output_tokens: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListAvailableModelsResponse {
    #[serde(default)]
    models: Vec<UpstreamModelInfo>,
}

/// 解析 ListAvailableModels 响应体（可单测，不联网）
pub fn parse_list_available_models_body(body: &str) -> anyhow::Result<Vec<UpstreamModelInfo>> {
    let parsed: ListAvailableModelsResponse = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("decode ListAvailableModels: {}", e))?;
    Ok(parsed
        .models
        .into_iter()
        .filter(|m| !m.model_id.trim().is_empty())
        .collect())
}

fn model_key(id: &str) -> String {
    id.trim().to_lowercase()
}

/// 按 modelId（大小写不敏感）去重合并，补齐空字段与 inputTypes
pub fn merge_unique_models(
    existing: &[UpstreamModelInfo],
    incoming: &[UpstreamModelInfo],
) -> Vec<UpstreamModelInfo> {
    if incoming.is_empty() {
        return existing.to_vec();
    }
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut merged: Vec<UpstreamModelInfo> = existing.to_vec();
    for (i, m) in merged.iter().enumerate() {
        index.insert(model_key(&m.model_id), i);
    }
    for model in incoming {
        let key = model_key(&model.model_id);
        if key.is_empty() {
            continue;
        }
        if let Some(&idx) = index.get(&key) {
            merged[idx] = merge_model_info(&merged[idx], model);
        } else {
            index.insert(key, merged.len());
            merged.push(model.clone());
        }
    }
    merged
}

fn merge_model_info(base: &UpstreamModelInfo, extra: &UpstreamModelInfo) -> UpstreamModelInfo {
    let mut out = base.clone();
    if out.model_name.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        out.model_name = extra.model_name.clone();
    }
    if out.description.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        out.description = extra.description.clone();
    }
    if out.rate_multiplier.unwrap_or(0.0) == 0.0 {
        out.rate_multiplier = extra.rate_multiplier;
    }
    if out.token_limits.is_none() {
        out.token_limits = extra.token_limits.clone();
    }
    out.input_types = merge_string_lists(&out.input_types, &extra.input_types);
    out
}

fn merge_string_lists(base: &[String], extra: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for item in base.iter().chain(extra.iter()) {
        let key = item.trim().to_lowercase();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        merged.push(item.clone());
    }
    merged
}

/// 规范化 model id 集合（lower-case keys）
pub fn model_id_set(models: &[UpstreamModelInfo]) -> HashSet<String> {
    models
        .iter()
        .map(|m| model_key(&m.model_id))
        .filter(|s| !s.is_empty())
        .collect()
}

/// 判断集合是否包含目标模型（空集合 = 未就绪，由调用方乐观处理）
pub fn set_contains_model(set: &HashSet<String>, model: &str) -> bool {
    let key = model_key(model);
    if set.contains(&key) {
        return true;
    }
    // 兼容客户端常见 4-6 / 4.6 写法差异（上游多为点号）
    for alt in model_id_aliases(&key) {
        if set.contains(&alt) {
            return true;
        }
    }
    false
}

fn model_id_aliases(key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let dotted = key
        .replace("4-8", "4.8")
        .replace("4-7", "4.7")
        .replace("4-6", "4.6")
        .replace("4-5", "4.5");
    if dotted != key {
        out.push(dotted);
    }
    let dashed = key
        .replace("4.8", "4-8")
        .replace("4.7", "4-7")
        .replace("4.6", "4-6")
        .replace("4.5", "4-5");
    if dashed != key {
        out.push(dashed);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_list_available_models_body_ok() {
        let body = r#"{"models":[{"modelId":"claude-sonnet-4.6","modelName":"Claude Sonnet 4.6","description":"desc","supportedInputTypes":["TEXT","IMAGE"],"rateMultiplier":1.0,"tokenLimits":{"maxInputTokens":1000000,"maxOutputTokens":64000}},{"modelId":"  ","modelName":"skip"}]}"#;
        let models = parse_list_available_models_body(body).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_id, "claude-sonnet-4.6");
        assert_eq!(models[0].model_name.as_deref(), Some("Claude Sonnet 4.6"));
        assert_eq!(models[0].input_types, vec!["TEXT", "IMAGE"]);
        assert_eq!(
            models[0].token_limits.as_ref().unwrap().max_output_tokens,
            Some(64000)
        );
    }

    #[test]
    fn parse_list_available_models_body_invalid_json() {
        let err = parse_list_available_models_body("{not json")
            .unwrap_err()
            .to_string();
        assert!(err.contains("decode ListAvailableModels"));
    }

    #[test]
    fn merge_unique_models_dedup_and_fill() {
        let a = vec![UpstreamModelInfo {
            model_id: "Claude-Sonnet-4.6".into(),
            model_name: None,
            description: Some("a".into()),
            input_types: vec!["TEXT".into()],
            rate_multiplier: None,
            token_limits: None,
        }];
        let b = vec![UpstreamModelInfo {
            model_id: "claude-sonnet-4.6".into(),
            model_name: Some("Sonnet".into()),
            description: Some("b".into()),
            input_types: vec!["IMAGE".into()],
            rate_multiplier: Some(1.5),
            token_limits: Some(TokenLimits {
                max_input_tokens: Some(1),
                max_output_tokens: Some(2),
            }),
        }];
        let merged = merge_unique_models(&a, &b);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].model_name.as_deref(), Some("Sonnet"));
        assert_eq!(merged[0].description.as_deref(), Some("a"));
        assert_eq!(
            merged[0].input_types,
            vec!["TEXT".to_string(), "IMAGE".to_string()]
        );
        assert_eq!(merged[0].rate_multiplier, Some(1.5));
        assert!(merged[0].token_limits.is_some());
    }

    #[test]
    fn model_id_set_lowercases() {
        let models = vec![UpstreamModelInfo {
            model_id: "Claude-Opus-4.6".into(),
            model_name: None,
            description: None,
            input_types: vec![],
            rate_multiplier: None,
            token_limits: None,
        }];
        let set = model_id_set(&models);
        assert!(set_contains_model(&set, "claude-opus-4.6"));
        assert!(!set_contains_model(&set, "claude-sonnet-4.6"));
    }

    #[test]
    fn set_contains_model_accepts_dash_version() {
        let models = vec![UpstreamModelInfo {
            model_id: "claude-sonnet-4.6".into(),
            model_name: None,
            description: None,
            input_types: vec![],
            rate_multiplier: None,
            token_limits: None,
        }];
        let set = model_id_set(&models);
        assert!(set_contains_model(&set, "claude-sonnet-4-6"));
        assert!(set_contains_model(&set, "Claude-Sonnet-4.6"));
    }

}
