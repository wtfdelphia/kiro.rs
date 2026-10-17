# Bridge Plan: sync-upstream-fable-and-opus55

生成时点：实现前。依据：proposal / design / tasks / specs 四工件（均 done，
`openspec validate --strict` 通过）+ AGENTS.md + openspec/project.md。

## 范围与非目标

范围：把上游三块净增量按语义移植进 dev 解析架构（fable 归一、opus-5.5 归一、
1M 上下文、fable adaptive thinking、静态 fallback、tool_result 图片修复、
README 映射表、测试）。

非目标：不执行真实 `git merge`/cherry-pick；不引入上游版本号与
`quality-gates.yaml`；不移植上游 `22f36e6` 负向断言与「常见排错」README 章节；
不触碰凭据、认证、Admin、OpenAI 协议层。

关键设计决策（详见 design.md）：语义移植（决策 1）；fable 无条件映射置首
（决策 2）；opus-5.5 先于 opus-5 判定（决策 3）；`extract_tool_result_content`
改签名（决策 4）；静态条目用连字符展示 id（决策 5）；不移植上游负向断言
（决策 6）。

## 高风险项

1. 归一分支顺序：fable 置于 sonnet/opus 之前，opus-5.5 先于 opus-5。顺序错会
   静默吞模型。缓解：正反向断言测试（任务 2.1/2.2/6.1）。
2. `get_context_window_size` 是跨协议共享函数：`src/openai/stream.rs:164`、
   `src/openai/responses_stream.rs:181`、`src/openai/handlers.rs:440` 均在用。
   加白名单只放宽不收紧，无回归风险，但全量测试必须覆盖（任务 6.3）。
3. `extract_tool_result_content` 签名变更只有一个调用方
   `process_message_content`（CodeGraph callers 确认），改动面封闭。
4. 上游 fable 条目 `created: 1756684800` 的注释（"Sep 1, 2026"）与实测日期
   （2025-09-01）不符。沿用时间戳值，注释按真实日期写。

## CodeGraph 证据

- `codegraph status`：索引可用（204 文件，3714 节点，11573 边）。
- `codegraph callers override_thinking_from_model_name`：6 个，含
  `post_messages`（handlers.rs:392）、`post_messages_cc`（handlers.rs:1026）
  与两个既有测试（opus_5_thinking_uses_adaptive_and_high_effort、
  opus_4_5_thinking_stays_enabled_without_output_config）。新增测试照此模式写。
- `codegraph callers extract_tool_result_content`：仅 `process_message_content`
  （converter.rs:602），签名变更影响面封闭。

## rg / 源码补盲

- `rg 'map_model|get_context_window_size|normalize_claude_model' src/openai/`：
  OpenAI 兼容层只消费 `get_context_window_size` 与 `override_thinking_from_model_name`，
  不直接调 `normalize_claude_model`。归一改动不波及 OpenAI 层，上下文窗口改动
  需全量回归确认。
- `rg 'claude-opus-5' .github/ Dockerfile docker-compose.yaml`：无命中，
  CI/发布资产与模型清单解耦，本变更无需改。
- `rg -n 'normalize_claude_model' src/anthropic/converter.rs:181`：目标函数定位。
  现有结构：base 剥 thinking 后缀 → sonnet → opus → haiku → None。fable 分支
  插在 base 判定链最前，与上游一致。
- `src/anthropic/handlers.rs:829`（`is_adaptive_thinking`）：现有判定含
  `opus-5` 包含匹配，已覆盖 5.5（本地试算 `"opus-5" in "claude-opus-5-5-thinking"`
  为真），只需追加 `fable`。
- `static_fallback_models`（handlers.rs:222）：既有测试
  `static_fallback_models_has_core_ids`（:1300）与
  `static_fallback_models_all_mappable`（:1313）断言结构确认，连字符 id 可过
  `map_model`（归一兼容连字符输入），扩展即可。

## 任务到执行步骤表

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1-1.3 基线 | 记录 `cargo check --release --all-targets` 告警数与 `cargo test` 基线；确认无凭据文件 | 输出存档 | 告警基线异常则停 |
| 2.1 fable 归一 | `normalize_claude_model` 插入 fable 分支 | 单测：连字符/点号均归一 | 编译错 |
| 2.2 opus-5.5 归一 | opus 分支插 5.5 判定于 opus-5 前 | 单测：四种拼写 + 反向断言 | 编译错 |
| 3.1 上下文窗口 | 白名单加两 id | 单测返回 1_000_000 | 既有 1M 用例回归 |
| 3.2 adaptive thinking | `is_adaptive_thinking` 加 `fable` | 单测：fable-thinking → adaptive + high effort | opus-4-5 反向用例回归 |
| 4.1-4.2 图片修复 | `extract_tool_result_content` 改签名；`process_message_content` 合并 images | 单测：含/不含 image 块 | `process_message_content` 其他分支回归 |
| 5.1 静态条目 | 四条连字符 id，created 取上游值，注释按真实日期 | 扩展两个既有测试 | 测试失败 |
| 5.2 README | 映射表补两行 | 人工比对归一顺序 | 无 |
| 6.1-6.3 测试 | 正向断言 + 反向断言 + 全量回归 | `cargo test --release --locked` | 任何失败 |
| 7.1-7.2 收尾 | 告警门禁 + validate + `git status --short` | 零新增告警；无凭据 | 告警新增 |
| 8.1-8.4 门禁 | 依次跑四个门禁 skill | 各产出报告 | 门禁失败 |

## 必跑验证

- `cargo check --release --all-targets`：零新增告警（硬门槛，对照基线）。
- `cargo test --release --locked`：全量通过，记录相对基线的数量变化。
- `openspec validate --all`：30 项通过。
- `git status --short`：无凭据、无 `.codegraph/`。

## README/AGENTS/spec 同步判断

- README：需同步（模型映射表），任务 5.2。
- AGENTS.md：无需修改。不引入新命令、新纪律或新入口。
- `spec/`：无需修改。`spec/requirements.md` 的模型映射描述若含具体清单，
  实现时核对（预检未见硬编码清单，实现中确认）。
- `openspec/specs/model-resolution/`：归档时由工具同步，本阶段不手改。

## 停止条件

- `cargo check --release --all-targets` 出现新增告警且无法在不引入
  坏味道的前提下消除。
- 全量测试出现与本变更无关的既有失败（记录后与用户确认，不擅自改无关代码）。
- fable/opus-5.5 归一行为与规格场景冲突且无法解释。
- 发现工作区有会被提交的真实凭据。
