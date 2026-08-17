//! Responses 工具形状归一：五种 wire 形状 -> 普通 function tool
//!
//! 上游 Kiro 的工具模型只有 `toolSpecification`，没有 custom / namespace 概念
//! （Kiro IDE 官方扩展同样在客户端侧把一切降级成 function + JSON Schema）。
//! 本层照该方向处理，不引入上游不存在的协议概念。
//!
//! 事实依据与 wire 实测见 `docs/codex-responses-lite-wire-analysis.md`。

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};

use super::error::OpenAiError;

/// 展平名分隔符：双下划线
///
/// 单下划线不可逆——工具名本身含下划线时（`spawn_agent`）无法反推分割点。
/// 与 Codex 自身的 `code_mode_name_for_tool_name` 一致。
const NAMESPACE_SEP: &str = "__";

/// freeform 工具降级后的固定 schema
fn freeform_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "input": {
                "type": "string",
                "description": "The raw input for this tool, passed through verbatim."
            }
        },
        "required": ["input"]
    })
}

/// 插在 freeform 工具 description 最前的调用约定说明
///
/// 必需：`exec` 这类工具的原始 description 会明确写「Accepts raw JavaScript source
/// text, not JSON」，与降级后必须回 JSON `arguments` 直接冲突。不加这段说明，
/// 模型会在两个矛盾的指令之间选错。
const FREEFORM_INVOCATION_NOTE: &str = "[Invocation note: call this tool as a normal JSON function call and put the raw tool input verbatim in the `input` field. The guidance below describes the format of that raw input, not the shape of this function call.]";

/// 响应侧还原所需的状态
///
/// 由归一层产出，随请求传递到响应侧。**两项都是静默错误源**：
/// 漏传不会有编译错误，只表现为客户端拒绝自己的工具调用或匹配不到工具。
///
/// key 用**降级/展平后的工具名**，不用缩短名：超长名的缩短与还原由既有的
/// `tool_name_map` 负责（`handlers.rs:335-339` / `responses_stream.rs:324-326`
/// 都在产出调用前已还原），响应侧拿到的是展平名。
#[derive(Debug, Clone, Default)]
pub struct ToolRewriteMap {
    /// 降级前的 custom 工具名，命中者响应侧须回 `custom_tool_call` 而非 `function_call`
    pub freeform: HashSet<String>,
    /// 展平名 -> (namespace, 原名)
    pub namespaces: HashMap<String, (String, String)>,
}


/// 展平名：`<namespace>__<name>`
pub fn flatten_namespace_name(namespace: &str, name: &str) -> String {
    format!("{}{}{}", namespace, NAMESPACE_SEP, name)
}

/// 递归删除 schema 中的 `encrypted` 键
///
/// Codex 的 Responses-only 标记（`codex-rs/tools/src/json_schema.rs`），
/// Kiro 上游无此概念。作用域仅限本归一层，不动共享的 `normalize_json_schema`。
pub fn strip_encrypted(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("encrypted");
            for v in map.values_mut() {
                strip_encrypted(v);
            }
        }
        Value::Array(items) => {
            for v in items {
                strip_encrypted(v);
            }
        }
        _ => {}
    }
}

/// 读取工具的参数 schema
///
/// 兼容 Chat 的嵌套形状（`function.parameters`）与 Responses 的顶层 `parameters`。
fn tool_parameters(tool: &Map<String, Value>) -> Option<Value> {
    tool.get("function")
        .and_then(|f| f.get("parameters"))
        .or_else(|| tool.get("parameters"))
        .cloned()
}

fn str_field(tool: &Map<String, Value>, key: &str) -> String {
    tool.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// namespace 的内层工具列表：优先 `tools`，回落 `children`
fn namespace_children(tool: &Map<String, Value>) -> Vec<Value> {
    tool.get("tools")
        .or_else(|| tool.get("children"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

/// 五种 wire 形状归一为普通 function tool
///
/// 返回归一后的工具数组与响应侧还原所需的映射。
/// 命名冲突返回 `InvalidRequest`（400）——冲突是客户端的命名问题，
/// 自动改名会让同一工具在不同轮次得到不同名字，客户端无法预期。
pub fn normalize_tools(tools: Vec<Value>) -> Result<(Vec<Value>, ToolRewriteMap), OpenAiError> {
    let mut out: Vec<Value> = Vec::with_capacity(tools.len());
    let mut rewrite = ToolRewriteMap::default();
    // 展平名 -> (namespace, 原名)，用于检测两个 namespace 展平到同名
    let mut flattened_seen: HashMap<String, (String, String)> = HashMap::new();

    // 顶层已声明的名字，用于检测展平名与顶层工具冲突
    let mut top_level: HashSet<String> = HashSet::new();
    for raw in &tools {
        let Some(obj) = raw.as_object() else { continue };
        let tool_type = str_field(obj, "type");
        if tool_type == "namespace" {
            continue;
        }
        let name = tool_name_of(obj);
        if !name.is_empty() {
            top_level.insert(name);
        }
    }

    for raw in tools {
        let Some(obj) = raw.as_object() else {
            tracing::warn!("工具定义不是对象，已丢弃");
            continue;
        };
        let tool_type = str_field(obj, "type");

        match tool_type.as_str() {
            "namespace" => {
                let namespace = str_field(obj, "name");
                if namespace.is_empty() {
                    tracing::warn!("namespace 工具缺少 name，已丢弃整组");
                    continue;
                }
                for child in namespace_children(obj) {
                    let Some(child_obj) = child.as_object() else {
                        continue;
                    };
                    let child_type = str_field(child_obj, "type");
                    let is_custom_child = child_type == "custom";
                    if !child_type.is_empty() && child_type != "function" && !is_custom_child {
                        tracing::warn!(
                            namespace = %namespace,
                            tool_type = %child_type,
                            "namespace 内层工具形状不受支持（仅 function / custom），已丢弃"
                        );
                        continue;
                    }
                    let child_name = str_field(child_obj, "name");
                    if child_name.is_empty() {
                        tracing::warn!(namespace = %namespace, "namespace 内层工具缺少 name，已丢弃");
                        continue;
                    }

                    let flat = flatten_namespace_name(&namespace, &child_name);
                    if top_level.contains(&flat) {
                        return Err(OpenAiError::InvalidRequest(format!(
                            "namespace tool \"{}\"/\"{}\" flattens to \"{}\" which conflicts with \
                             a top-level tool of the same name; this upstream cannot disambiguate \
                             them, rename one of the tools",
                            namespace, child_name, flat
                        )));
                    }
                    if let Some((prev_ns, prev_name)) = flattened_seen.get(&flat) {
                        if prev_ns != &namespace || prev_name != &child_name {
                            return Err(OpenAiError::InvalidRequest(format!(
                                "namespace tools \"{}\"/\"{}\" and \"{}\"/\"{}\" both flatten to \
                                 \"{}\"; this upstream cannot disambiguate them, rename one of \
                                 the tools",
                                prev_ns, prev_name, namespace, child_name, flat
                            )));
                        }
                        // 同一工具重复出现，跳过而非报错
                        continue;
                    }
                    flattened_seen
                        .insert(flat.clone(), (namespace.clone(), child_name.clone()));
                    rewrite
                        .namespaces
                        .insert(flat.clone(), (namespace.clone(), child_name.clone()));

                    if is_custom_child {
                        // 内层 custom：与顶层 custom 相同的 freeform 降级，
                        // 但以展平名注册进 freeform 集合，响应侧按两级映射还原。
                        rewrite.freeform.insert(flat.clone());
                        out.push(json!({
                            "type": "function",
                            "name": flat,
                            "description": freeform_description(child_obj),
                            "parameters": freeform_schema(),
                        }));
                    } else {
                        let mut params = tool_parameters(child_obj).unwrap_or_else(|| json!({}));
                        strip_encrypted(&mut params);
                        out.push(json!({
                            "type": "function",
                            "name": flat,
                            "description": str_field(child_obj, "description"),
                            "parameters": params,
                        }));
                    }
                }
            }
            "custom" => {
                let name = str_field(obj, "name");
                if name.is_empty() {
                    tracing::warn!(tool_type = "custom", "工具缺少 name，已丢弃");
                    continue;
                }
                rewrite.freeform.insert(name.clone());
                out.push(json!({
                    "type": "function",
                    "name": name,
                    "description": freeform_description(obj),
                    "parameters": freeform_schema(),
                }));
            }
            _ => {
                let name = tool_name_of(obj);
                if name.is_empty() {
                    tracing::warn!(
                        tool_type = %tool_type,
                        "工具缺少 name，已丢弃（web_search / tool_search 等无名形状不被支持）"
                    );
                    continue;
                }
                let mut params = tool_parameters(obj).unwrap_or_else(|| json!({}));
                strip_encrypted(&mut params);
                out.push(json!({
                    "type": "function",
                    "name": name,
                    "description": tool_description_of(obj),
                    "parameters": params,
                }));
            }
        }
    }

    Ok((out, rewrite))
}

/// 工具名：兼容 Chat 嵌套形状与 Responses 顶层形状
fn tool_name_of(tool: &Map<String, Value>) -> String {
    tool.get("function")
        .and_then(|f| f.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| str_field(tool, "name"))
}

fn tool_description_of(tool: &Map<String, Value>) -> String {
    tool.get("function")
        .and_then(|f| f.get("description"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            tool.get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        })
}

/// freeform 工具的 description：调用约定说明 + 原描述 + 语法定义
///
/// 语法定义（lark 文法）追加到末尾——上游无 grammar 概念，
/// description 是唯一能让模型看到该约束的位置。
fn freeform_description(tool: &Map<String, Value>) -> String {
    let original = tool
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut desc = String::with_capacity(FREEFORM_INVOCATION_NOTE.len() + original.len() + 256);
    desc.push_str(FREEFORM_INVOCATION_NOTE);
    if !original.is_empty() {
        desc.push_str("\n\n");
        desc.push_str(original);
    }

    if let Some(format) = tool.get("format").and_then(|v| v.as_object()) {
        let definition = format
            .get("definition")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !definition.is_empty() {
            let syntax = format.get("syntax").and_then(|v| v.as_str()).unwrap_or("");
            desc.push_str("\n\nThe raw input must conform to the following");
            if !syntax.is_empty() {
                desc.push(' ');
                desc.push_str(syntax);
            }
            desc.push_str(" grammar:\n");
            desc.push_str(definition);
        }
    }

    desc
}

/// freeform 工具调用的 `input` 提取
///
/// 降级后模型被告知这是 `{input: string}` 的 function tool，但未必老实包装——
/// `exec` 的 description 明确要求裸源码，模型很可能直接回裸文本。
/// 分支顺序敏感。
pub fn extract_custom_input(arguments: &str) -> String {
    if arguments.trim().is_empty() {
        return String::new();
    }
    let Ok(parsed) = serde_json::from_str::<Value>(arguments) else {
        // 非法 JSON：模型直接回了裸输入
        return arguments.to_string();
    };
    match parsed.get("input") {
        Some(Value::String(s)) => s.clone(),
        // 有 input 键但不是字符串：无法判定，原样回
        Some(_) => arguments.to_string(),
        None => {
            let is_empty_object = parsed.as_object().map(|o| o.is_empty()).unwrap_or(false);
            if is_empty_object {
                String::new()
            } else {
                arguments.to_string()
            }
        }
    }
}

/// `tool_choice` 改写：客户端方言 -> 上游可识别形状
pub fn rewrite_tool_choice(choice: &Value) -> Option<Value> {
    let obj = choice.as_object()?;
    match str_field(obj, "type").as_str() {
        "custom" => {
            let name = str_field(obj, "name");
            if name.is_empty() {
                Some(json!("auto"))
            } else {
                Some(json!({"type": "function", "name": name}))
            }
        }
        // namespace 不是单个工具，无法表达为具体选择
        "namespace" => Some(json!("auto")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_encrypted_nested() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "message": {"type": "string", "encrypted": true},
                "nested": {
                    "type": "object",
                    "properties": {"deep": {"type": "string", "encrypted": true}}
                },
                "arr": [{"encrypted": true, "type": "string"}]
            },
            "encrypted": true
        });
        strip_encrypted(&mut schema);
        let s = serde_json::to_string(&schema).unwrap();
        assert!(!s.contains("encrypted"), "encrypted 应被递归删除: {}", s);
        // 其余字段不受影响
        assert_eq!(schema["properties"]["message"]["type"], "string");
        assert_eq!(schema["properties"]["nested"]["properties"]["deep"]["type"], "string");
    }

    #[test]
    fn test_flatten_namespace_name_uses_double_underscore() {
        assert_eq!(
            flatten_namespace_name("collaboration", "spawn_agent"),
            "collaboration__spawn_agent"
        );
    }

    // === custom(freeform) 降级 ===

    fn exec_tool() -> Value {
        json!({
            "type": "custom",
            "name": "exec",
            "description": "Run JavaScript code to orchestrate tool calls\n\
                - Accepts raw JavaScript source text, not JSON, quoted strings, \
                or markdown code fences.",
            "format": {
                "type": "grammar",
                "syntax": "lark",
                "definition": "start: pragma_source | plain_source\nSOURCE: /[\\s\\S]+/"
            }
        })
    }

    #[test]
    fn test_custom_downgraded_to_input_string_schema() {
        let (out, rewrite) = normalize_tools(vec![exec_tool()]).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["type"], "function");
        assert_eq!(out[0]["name"], "exec");

        let params = &out[0]["parameters"];
        assert_eq!(params["type"], "object");
        assert_eq!(params["properties"]["input"]["type"], "string");
        assert_eq!(params["required"][0], "input");

        assert!(rewrite.freeform.contains("exec"), "须记入 freeform 集合");
    }

    /// 调用约定说明必须在最前：原 description 明确要求「不要用 JSON」，
    /// 而降级后模型必须回 JSON arguments，两者直接冲突。
    #[test]
    fn test_custom_description_starts_with_invocation_note() {
        let (out, _) = normalize_tools(vec![exec_tool()]).unwrap();
        let desc = out[0]["description"].as_str().unwrap();
        assert!(
            desc.starts_with("[Invocation note:"),
            "调用约定说明须置于最前: {}",
            &desc[..desc.len().min(80)]
        );
        assert!(
            desc.contains("`input` field"),
            "须说明 input 字段承载原始输入"
        );
        assert!(
            desc.contains("Accepts raw JavaScript source text"),
            "原始描述须保留"
        );
    }

    #[test]
    fn test_custom_grammar_definition_preserved() {
        let (out, _) = normalize_tools(vec![exec_tool()]).unwrap();
        let desc = out[0]["description"].as_str().unwrap();
        assert!(desc.contains("SOURCE: /[\\s\\S]+/"), "lark 文法须出现在描述中");
        assert!(desc.contains("lark"), "须点明语法名");
    }

    /// 锁定：freeform 集合的 key 是工具原名，**不做缩短**
    ///
    /// 超长名的缩短与还原由既有 `tool_name_map` 负责，响应侧在查本集合时
    /// 名字已被还原。若此处存缩短名，超长工具的还原会静默失效——
    /// 编译器不报，表现为客户端拒绝自己的工具调用、模型反复重试。
    #[test]
    fn test_custom_freeform_key_is_original_name_not_shortened() {
        let long_name = format!("exec_{}", "x".repeat(80));
        let mut tool = exec_tool();
        tool["name"] = json!(long_name.clone());
        let (_, rewrite) = normalize_tools(vec![tool]).unwrap();

        assert!(
            rewrite.freeform.contains(&long_name),
            "集合 key 应为原名（响应侧查表时名字已由 tool_name_map 还原），实际: {:?}",
            rewrite.freeform
        );
        assert_eq!(rewrite.freeform.len(), 1);
    }

    #[test]
    fn test_custom_without_name_dropped() {
        let (out, rewrite) = normalize_tools(vec![json!({"type":"custom","description":"d"})]).unwrap();
        assert!(out.is_empty());
        assert!(rewrite.freeform.is_empty());
    }

    #[test]
    fn test_custom_without_format_still_downgrades() {
        let (out, _) = normalize_tools(vec![json!({
            "type":"custom","name":"apply_patch","description":"Apply a patch"
        })])
        .unwrap();
        assert_eq!(out[0]["parameters"]["properties"]["input"]["type"], "string");
        let desc = out[0]["description"].as_str().unwrap();
        assert!(desc.starts_with("[Invocation note:"));
        assert!(desc.contains("Apply a patch"));
    }

    // === namespace 展平 ===

    fn collaboration_tool() -> Value {
        json!({
            "type": "namespace",
            "name": "collaboration",
            "description": "Tools for spawning and managing sub-agents.",
            "tools": [
                {"type":"function","name":"spawn_agent","description":"Spawn","strict":false,
                 "parameters":{"type":"object","properties":{
                    "message":{"type":"string","encrypted":true}},"required":["message"]}},
                {"type":"function","name":"wait_agent","description":"Wait","strict":false,
                 "parameters":{"type":"object","properties":{"target":{"type":"string"}}}}
            ]
        })
    }

    #[test]
    fn test_namespace_flattened_into_independent_tools() {
        let (out, rewrite) = normalize_tools(vec![collaboration_tool()]).unwrap();
        let names: Vec<&str> = out.iter().map(|t| t["name"].as_str().unwrap()).collect();

        assert_eq!(names, vec!["collaboration__spawn_agent", "collaboration__wait_agent"]);
        // 不应残留名为 collaboration 的空壳
        assert!(!names.contains(&"collaboration"));

        // schema 完整保留（除 encrypted）
        assert_eq!(out[0]["parameters"]["properties"]["message"]["type"], "string");
        assert_eq!(out[0]["parameters"]["required"][0], "message");

        assert_eq!(
            rewrite.namespaces.get("collaboration__spawn_agent"),
            Some(&("collaboration".to_string(), "spawn_agent".to_string()))
        );
    }

    #[test]
    fn test_namespace_children_strip_encrypted() {
        let (out, _) = normalize_tools(vec![collaboration_tool()]).unwrap();
        let s = serde_json::to_string(&out).unwrap();
        assert!(!s.contains("encrypted"), "内层 schema 的 encrypted 须剔除: {}", s);
    }

    #[test]
    fn test_namespace_children_field_falls_back_to_children() {
        let (out, _) = normalize_tools(vec![json!({
            "type":"namespace","name":"ns",
            "children":[{"type":"function","name":"t","parameters":{"type":"object"}}]
        })])
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], "ns__t");
    }

    #[test]
    fn test_flattened_name_conflicts_with_top_level_tool() {
        let err = normalize_tools(vec![
            json!({"type":"function","name":"collaboration__spawn_agent",
                   "parameters":{"type":"object"}}),
            collaboration_tool(),
        ])
        .unwrap_err();

        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        let msg = err.message();
        assert!(msg.contains("collaboration"), "错误须含 namespace 名: {}", msg);
        assert!(msg.contains("spawn_agent"), "错误须含工具名: {}", msg);
        assert!(
            msg.contains("collaboration__spawn_agent"),
            "错误须含冲突的展平名: {}",
            msg
        );
        assert!(msg.contains("rename"), "错误须给出处置建议: {}", msg);
    }

    #[test]
    fn test_two_namespaces_flatten_to_same_name() {
        // ns 下的 a__b 与 ns__a 下的 b 都展平成 ns__a__b
        let err = normalize_tools(vec![
            json!({"type":"namespace","name":"ns",
                   "tools":[{"type":"function","name":"a__b","parameters":{"type":"object"}}]}),
            json!({"type":"namespace","name":"ns__a",
                   "tools":[{"type":"function","name":"b","parameters":{"type":"object"}}]}),
        ])
        .unwrap_err();

        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        let msg = err.message();
        assert!(msg.contains("ns__a__b"), "错误须含冲突的展平名: {}", msg);
        assert!(msg.contains("both flatten to"), "错误须说明冲突性质: {}", msg);
    }

    #[test]
    fn test_duplicate_same_tool_in_namespace_skipped_not_error() {
        // 同一 (namespace, name) 重复出现属幂等，不应报错
        let (out, _) = normalize_tools(vec![json!({
            "type":"namespace","name":"ns","tools":[
                {"type":"function","name":"t","parameters":{"type":"object"}},
                {"type":"function","name":"t","parameters":{"type":"object"}}
            ]
        })])
        .unwrap();
        assert_eq!(out.len(), 1, "重复工具应去重");
    }

    /// 锁定：逆映射 key 是展平名，**不做缩短**（理由同 freeform 集合）
    #[test]
    fn test_namespace_reverse_map_key_is_flattened_name_not_shortened() {
        let long_child = "c".repeat(80);
        let flat = flatten_namespace_name("ns", &long_child);

        let (_, rewrite) = normalize_tools(vec![json!({
            "type":"namespace","name":"ns",
            "tools":[{"type":"function","name":long_child,"parameters":{"type":"object"}}]
        })])
        .unwrap();

        assert_eq!(
            rewrite.namespaces.get(&flat),
            Some(&("ns".to_string(), long_child.clone())),
            "逆映射 key 应为展平名，实际: {:?}",
            rewrite.namespaces.keys().collect::<Vec<_>>()
        );
    }

    // === namespace 内层 custom（freeform）展平 ===

    fn functions_ns_with_custom() -> Value {
        json!({
            "type": "namespace",
            "name": "functions",
            "tools": [
                {"type":"function","name":"wait","description":"Wait","strict":false,
                 "parameters":{"type":"object","properties":{"seconds":{"type":"integer"}}}},
                {"type":"custom","name":"apply_patch","description":"Apply a patch to files",
                 "format":{"type":"grammar","syntax":"lark",
                           "definition":"start: patch\npatch: /BEGIN Patch/"}}
            ]
        })
    }

    #[test]
    fn test_namespace_inner_custom_flattened_and_downgraded() {
        let (out, _) = normalize_tools(vec![functions_ns_with_custom()]).unwrap();
        let names: Vec<&str> = out.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["functions__wait", "functions__apply_patch"]);

        let custom = &out[1];
        assert_eq!(custom["type"], "function");
        // freeform 降级 schema：单个必填字符串属性 input
        assert_eq!(custom["parameters"]["type"], "object");
        assert_eq!(custom["parameters"]["properties"]["input"]["type"], "string");
        assert_eq!(custom["parameters"]["required"][0], "input");

        // description：调用约定说明在最前，原描述与文法保留
        let desc = custom["description"].as_str().unwrap();
        assert!(desc.starts_with("[Invocation note:"), "调用约定说明须置于最前");
        assert!(desc.contains("Apply a patch to files"), "原始描述须保留");
        assert!(desc.contains("BEGIN Patch"), "文法定义须保留");
    }

    /// 锁定：展平名同时进两级映射——漏 freeform 会让调用回成 function_call，
    /// 漏 namespaces 会让客户端按 (namespace, name) 匹配失败。
    #[test]
    fn test_namespace_inner_custom_registered_in_both_maps() {
        let (_, rewrite) = normalize_tools(vec![functions_ns_with_custom()]).unwrap();
        let flat = "functions__apply_patch";
        assert!(rewrite.freeform.contains(flat), "展平名须记入 freeform 集合");
        assert_eq!(
            rewrite.namespaces.get(flat),
            Some(&("functions".to_string(), "apply_patch".to_string())),
            "展平名须记入 namespace 逆映射"
        );
    }

    #[test]
    fn test_namespace_inner_custom_conflicts_with_top_level() {
        let err = normalize_tools(vec![
            json!({"type":"function","name":"functions__apply_patch",
                   "parameters":{"type":"object"}}),
            functions_ns_with_custom(),
        ])
        .unwrap_err();

        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        let msg = err.message();
        assert!(msg.contains("functions__apply_patch"), "错误须含冲突的展平名: {}", msg);
        assert!(msg.contains("apply_patch"), "错误须含内层工具名: {}", msg);
        assert!(msg.contains("rename"), "错误须给出处置建议: {}", msg);
    }

    #[test]
    fn test_namespace_inner_custom_flatten_conflicts_with_other_flatten() {
        // functions 下的 ns__x 与 functions__ns 下的 x 展平同名
        let err = normalize_tools(vec![
            json!({"type":"namespace","name":"functions",
                   "tools":[{"type":"custom","name":"ns__x","description":"d"}]}),
            json!({"type":"namespace","name":"functions__ns",
                   "tools":[{"type":"function","name":"x","parameters":{"type":"object"}}]}),
        ])
        .unwrap_err();

        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        let msg = err.message();
        assert!(msg.contains("functions__ns__x"), "错误须含冲突的展平名: {}", msg);
        assert!(msg.contains("both flatten to"), "错误须说明冲突性质: {}", msg);
    }

    #[test]
    fn test_namespace_inner_custom_without_name_dropped_others_unaffected() {
        let (out, rewrite) = normalize_tools(vec![json!({
            "type":"namespace","name":"functions","tools":[
                {"type":"function","name":"wait","parameters":{"type":"object"}},
                {"type":"custom","description":"no name"}
            ]
        })])
        .unwrap();

        assert_eq!(out.len(), 1, "缺名 custom 应被丢弃，其余内层不受影响");
        assert_eq!(out[0]["name"], "functions__wait");
        assert!(rewrite.freeform.is_empty(), "不得为缺名工具登记 freeform");
    }

    #[test]
    fn test_namespace_without_name_dropped() {
        let (out, _) = normalize_tools(vec![json!({
            "type":"namespace",
            "tools":[{"type":"function","name":"t","parameters":{"type":"object"}}]
        })])
        .unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn test_unnamed_tool_dropped_others_unaffected() {
        // web_search / tool_search 序列化后不带 name
        let (out, _) = normalize_tools(vec![
            json!({"type":"web_search"}),
            json!({"type":"function","name":"keep","parameters":{"type":"object"}}),
        ])
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], "keep");
    }

    // === extract_custom_input 五分支 ===

    #[test]
    fn test_extract_input_blank() {
        assert_eq!(extract_custom_input(""), "");
        assert_eq!(extract_custom_input("   \n "), "");
    }

    #[test]
    fn test_extract_input_invalid_json_returns_raw() {
        // exec 的描述要求裸源码，模型很可能照做
        let raw = "const x = 1;\nawait tools.exec_command({cmd: \"git status\"});";
        assert_eq!(extract_custom_input(raw), raw);
    }

    #[test]
    fn test_extract_input_with_input_key() {
        let args = r#"{"input":"const x = 1;"}"#;
        assert_eq!(extract_custom_input(args), "const x = 1;");
    }

    #[test]
    fn test_extract_input_empty_object() {
        assert_eq!(extract_custom_input("{}"), "");
    }

    #[test]
    fn test_extract_input_no_input_key_non_empty_returns_raw() {
        let args = r#"{"source":"const x = 1;"}"#;
        assert_eq!(extract_custom_input(args), args);
    }

    #[test]
    fn test_extract_input_non_string_input_returns_raw() {
        let args = r#"{"input":{"nested":true}}"#;
        assert_eq!(extract_custom_input(args), args);
    }

    #[test]
    fn test_extract_input_preserves_newlines_and_quotes() {
        let src = "const s = \"line1\\nline2\";\nif (s) {\n  text(\"ok\");\n}";
        let args = serde_json::to_string(&json!({"input": src})).unwrap();
        assert_eq!(extract_custom_input(&args), src, "换行与引号须无损");
    }

    // === tool_choice 改写 ===

    #[test]
    fn test_rewrite_tool_choice_custom() {
        let r = rewrite_tool_choice(&json!({"type":"custom","name":"exec"})).unwrap();
        assert_eq!(r, json!({"type":"function","name":"exec"}));
    }

    #[test]
    fn test_rewrite_tool_choice_namespace_becomes_auto() {
        let r = rewrite_tool_choice(&json!({"type":"namespace","name":"collaboration"})).unwrap();
        assert_eq!(r, json!("auto"));
    }

    #[test]
    fn test_rewrite_tool_choice_leaves_others_untouched() {
        assert!(rewrite_tool_choice(&json!({"type":"function","name":"f"})).is_none());
        assert!(rewrite_tool_choice(&json!("auto")).is_none());
    }
}
