//! 对外 Public API 端点注册表（单一事实源）
//!
//! 本模块只描述「客户端 -> 本代理」的端点（Public Client API）。
//! 「本代理 -> 上游 Kiro」的端点（由 `/api/admin/settings/endpoint` 管理）不属于此处，
//! 两者概念必须分开，详见 `openspec/changes/public-api-catalog-admin-display/design.md`。

/// 端点可用状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointStatus {
    /// 已挂载可用
    Live,
    /// 已挂载但契约可能变化
    // 保留原因：openspec/specs/public-api-catalog/spec.md:39 要求条目 status 必须可声明为 `beta`；
    // `as_str` 的 `"beta"` 分支用于 DTO 序列化（public_api/dto.rs），删除该 variant 会收窄已发布契约。
    #[allow(dead_code)]
    Beta,
    /// 已登记但未挂载，请求返回 404
    Planned,
}

impl EndpointStatus {
    /// DTO 序列化用的小写标识
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Beta => "beta",
            Self::Planned => "planned",
        }
    }
}

/// 端点鉴权方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    /// 客户端 apiKey（受 requireApiKey 约束，x-api-key 或 Bearer）
    ClientApiKey,
}

impl AuthKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClientApiKey => "clientApiKey",
        }
    }
}

/// 单个对外端点
#[derive(Debug, Clone, Copy)]
pub struct PublicEndpoint {
    /// 稳定标识，如 "openai.chat.completions"
    pub id: &'static str,
    /// 协议族，如 "claude" | "openai-chat" | "openai-responses" | "models"
    pub family: &'static str,
    pub method: &'static str,
    /// canonical 路径，别名不写在这里
    pub path: &'static str,
    /// 兼容别名；首版全为空（见 design.md D5）
    pub aliases: &'static [&'static str],
    pub auth: AuthKind,
    pub status: EndpointStatus,
    /// 是否支持流式响应
    pub stream: bool,
    pub summary: &'static str,
    /// 接入提示，展示在 Admin 面板
    pub client_hints: &'static [&'static str],
}

/// 协议族展示名
pub fn family_label(family: &str) -> &'static str {
    match family {
        "models" => "Models",
        "claude" => "Anthropic Messages",
        "openai-chat" => "OpenAI Chat Completions",
        "openai-responses" => "OpenAI Responses",
        _ => "Other",
    }
}

/// canonical 端点清单
///
/// live 项必须与 `src/anthropic/router.rs` 的实际挂载一致，
/// planned 项必须未挂载（由 `tests` 中的双向断言强制）。
pub const ENDPOINTS: &[PublicEndpoint] = &[
    PublicEndpoint {
        id: "models.list",
        family: "models",
        method: "GET",
        path: "/v1/models",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Live,
        stream: false,
        summary: "模型列表，Anthropic 与 OpenAI 客户端共用",
        client_hints: &[
            "需鉴权：受 requireApiKey 约束，未配置 key 的客户端探测会得到 401",
            "响应为 OpenAI list shape 的超集，OpenAI SDK 可直接消费",
        ],
    },
    PublicEndpoint {
        id: "claude.messages",
        family: "claude",
        method: "POST",
        path: "/v1/messages",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Live,
        stream: true,
        summary: "Anthropic Messages 主入口，流式为增量输出",
        client_hints: &[
            "ANTHROPIC_BASE_URL 不带 /v1 后缀",
            "流式 message_start 的 input_tokens 为估算值，真值随后在 message_delta 更新",
        ],
    },
    PublicEndpoint {
        id: "claude.count_tokens",
        family: "claude",
        method: "POST",
        path: "/v1/messages/count_tokens",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Live,
        stream: false,
        summary: "Token 计数",
        client_hints: &[],
    },
    PublicEndpoint {
        id: "claude.cc.messages",
        family: "claude",
        method: "POST",
        path: "/cc/v1/messages",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Live,
        stream: true,
        summary: "Claude Code 兼容入口，流式为缓冲输出",
        client_hints: &[
            "与 /v1/messages 的差异：缓冲流。全程只发 ping 保活，待上游 contextUsageEvent 到达后一次性吐出全部事件",
            "换来 message_start 中准确的 input_tokens，代价是失去增量体验",
        ],
    },
    PublicEndpoint {
        id: "claude.cc.count_tokens",
        family: "claude",
        method: "POST",
        path: "/cc/v1/messages/count_tokens",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Live,
        stream: false,
        summary: "Token 计数（与 /v1 复用同一 handler）",
        client_hints: &[],
    },
    PublicEndpoint {
        id: "openai.chat.completions",
        family: "openai-chat",
        method: "POST",
        path: "/v1/chat/completions",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Live,
        stream: true,
        summary: "OpenAI Chat Completions 兼容入口",
        client_hints: &[
            "OPENAI_BASE_URL 需带 /v1 后缀",
            "响应回显的 model 为客户端请求的原始名（如 gpt-4o），并非实际执行的 Claude 模型；按 model 归类计费的中间层需注意",
            "usage 需客户端传 stream_options.include_usage 才在流式末尾返回",
            "不支持服务端 web_search 工具，该能力仅在 /v1/responses 提供",
        ],
    },
    PublicEndpoint {
        id: "openai.responses",
        family: "openai-responses",
        method: "POST",
        path: "/v1/responses",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Live,
        stream: true,
        summary: "OpenAI Responses 兼容入口（无状态）",
        client_hints: &[
            "OPENAI_BASE_URL 需带 /v1 后缀",
            "无状态：携带 previous_response_id 将返回 400（请在 input 中带上完整对话），store 字段被忽略",
            "支持服务端 web_search 代执行；判定比 /v1/messages 宽（含 web_search_20250305 等形状），可在 Admin 运行时设置中关闭",
            "SSE 为命名语义事件（event: response.*），与 /v1/chat/completions 的纯 data 行不同",
            "响应回显的 model 为客户端请求的原始名，并非实际执行的 Claude 模型",
        ],
    },
    PublicEndpoint {
        id: "openai.responses.retrieve",
        family: "openai-responses",
        method: "GET",
        path: "/v1/responses/{id}",
        aliases: &[],
        auth: AuthKind::ClientApiKey,
        status: EndpointStatus::Planned,
        stream: false,
        summary: "按 id 读取 Responses 对象（需先实现有状态存储）",
        client_hints: &["依赖 previous_response_id 持久化能力，尚未规划实现"],
    },
];

/// 返回全部端点
pub fn catalog() -> &'static [PublicEndpoint] {
    ENDPOINTS
}

/// 返回 status 为 Live 的端点（启动日志用）
pub fn live_endpoints() -> impl Iterator<Item = &'static PublicEndpoint> {
    ENDPOINTS
        .iter()
        .filter(|e| e.status == EndpointStatus::Live)
}

/// 按登记顺序返回去重后的协议族列表
pub fn families() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for e in ENDPOINTS {
        if !out.contains(&e.family) {
            out.push(e.family);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_ids_unique() {
        let mut seen = HashSet::new();
        for e in ENDPOINTS {
            assert!(seen.insert(e.id), "重复的端点 id: {}", e.id);
        }
    }

    #[test]
    fn test_method_path_unique() {
        let mut seen = HashSet::new();
        for e in ENDPOINTS {
            let key = (e.method, e.path);
            assert!(seen.insert(key), "重复的 method+path: {} {}", e.method, e.path);
        }
    }

    #[test]
    fn test_live_entries_complete() {
        for e in live_endpoints() {
            assert!(!e.id.is_empty(), "live 端点 id 为空");
            assert!(!e.family.is_empty(), "live 端点 {} family 为空", e.id);
            assert!(!e.summary.is_empty(), "live 端点 {} summary 为空", e.id);
            assert!(
                e.path.starts_with('/'),
                "live 端点 {} path 未以 / 开头",
                e.id
            );
            assert!(
                matches!(e.method, "GET" | "POST"),
                "live 端点 {} method 非法: {}",
                e.id,
                e.method
            );
        }
    }

    #[test]
    fn test_aliases_empty_in_first_version() {
        for e in ENDPOINTS {
            assert!(
                e.aliases.is_empty(),
                "首版不支持路径别名，但 {} 登记了 {:?}",
                e.id,
                e.aliases
            );
        }
    }

    #[test]
    fn test_expected_live_set() {
        let live: HashSet<(&str, &str)> = live_endpoints().map(|e| (e.method, e.path)).collect();
        for expected in [
            ("GET", "/v1/models"),
            ("POST", "/v1/messages"),
            ("POST", "/v1/messages/count_tokens"),
            ("POST", "/cc/v1/messages"),
            ("POST", "/cc/v1/messages/count_tokens"),
            ("POST", "/v1/chat/completions"),
            ("POST", "/v1/responses"),
        ] {
            assert!(live.contains(&expected), "缺少 live 端点: {:?}", expected);
        }
        assert_eq!(live.len(), 7, "live 端点数量与预期不符: {:?}", live);
    }

    #[test]
    fn test_chat_completions_live() {
        let e = ENDPOINTS
            .iter()
            .find(|e| e.path == "/v1/chat/completions")
            .expect("未登记 /v1/chat/completions");
        assert_eq!(e.status, EndpointStatus::Live);
    }

    #[test]
    fn test_responses_live_retrieve_still_planned() {
        let live = ENDPOINTS
            .iter()
            .find(|e| e.path == "/v1/responses")
            .expect("未登记 /v1/responses");
        assert_eq!(live.status, EndpointStatus::Live);

        let retrieve = ENDPOINTS
            .iter()
            .find(|e| e.path == "/v1/responses/{id}")
            .expect("未登记 /v1/responses/{id}");
        assert_eq!(
            retrieve.status,
            EndpointStatus::Planned,
            "retrieve 需先实现持久化，应仍为 planned"
        );
    }

    #[test]
    fn test_responses_hints_document_stateless_and_websearch() {
        let e = ENDPOINTS
            .iter()
            .find(|e| e.path == "/v1/responses")
            .expect("未登记 /v1/responses");
        let hints = e.client_hints.join(" ");
        assert!(hints.contains("previous_response_id"), "应说明无状态限制");
        assert!(hints.contains("web_search"), "应说明 web_search 支持与差异");
        assert!(hints.contains("event:"), "应说明 SSE 为命名事件");
    }

    #[test]
    fn test_cc_stream_difference_documented() {
        let cc = ENDPOINTS
            .iter()
            .find(|e| e.path == "/cc/v1/messages")
            .expect("未登记 /cc/v1/messages");
        assert!(cc.stream, "/cc/v1/messages 应标记为流式");
        let hints = cc.client_hints.join(" ");
        assert!(
            hints.contains("/v1/messages") && hints.contains("缓冲"),
            "/cc/v1/messages 需在 client_hints 说明与 /v1/messages 的流式差异"
        );
    }

    #[test]
    fn test_models_auth_hint_present() {
        let models = ENDPOINTS
            .iter()
            .find(|e| e.path == "/v1/models")
            .expect("未登记 /v1/models");
        assert!(
            models.client_hints.iter().any(|h| h.contains("需鉴权")),
            "/v1/models 需标注需鉴权（design.md D6）"
        );
    }

    #[test]
    fn test_families_order() {
        assert_eq!(
            families(),
            vec!["models", "claude", "openai-chat", "openai-responses"]
        );
    }
}
