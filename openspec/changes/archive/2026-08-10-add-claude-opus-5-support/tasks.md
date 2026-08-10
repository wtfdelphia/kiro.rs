## 1. 基线与实施桥接

- [x] 1.1 运行 `openspec-superpowers-bridge`，记录规格到文件、风险、工具与验证命令的映射
- [x] 1.2 记录实现前基线：`cargo check --release --all-targets` 告警数、`normalize_claude_model("claude-opus-5")` 当前返回值
- [x] 1.3 确认 `config.json`、`credentials.*` 与 `.codegraph/` 不进入本次变更

## 2. 模型归一

- [x] 2.1 在 `normalize_claude_model` 的 opus 分支新增 `opus-5` 判定，置于所有 `4-x` 判定之前
- [x] 2.2 新增单测覆盖 `claude-opus-5`、`claude-opus-5-thinking` 归一为 `claude-opus-5`
- [x] 2.3 新增回归单测断言 `claude-opus-4-5-20251101` 仍归一为 `claude-opus-4.5`，未被 opus-5 截获

## 3. 上下文窗口与 thinking 策略

- [x] 3.1 将 `claude-opus-5` 纳入 `get_context_window_size` 的 1M 白名单，并更新其文档注释
- [x] 3.2 单测断言 `get_context_window_size("claude-opus-5")` 为 `1_000_000`
- [x] 3.3 将 `opus-5` 纳入 `override_thinking_from_model_name` 的 `is_adaptive_thinking` 集合
- [x] 3.4 单测断言 opus-5 请求的 thinking 类型为 adaptive 且附带 high effort output config

## 4. 公开模型列表

- [x] 4.1 在 `static_fallback_models` 补充 `claude-opus-5` 与 `claude-opus-5-thinking` 条目，字段风格与既有 Sonnet 5 条目对齐
- [x] 4.2 扩展 `static_fallback_models_has_core_ids` 断言覆盖两个新 id
- [x] 4.3 确认动态 catalog 路径未被改动，`models_from_catalog` 相关测试仍通过

## 5. 文档与规格同步

- [x] 5.1 在 README 模型映射表补充 `*opus-5*` → `claude-opus-5` 一行，位置置于 `*opus*` 各 4-x 行之前
- [x] 5.2 核对 AGENTS.md 无需修改并在最终报告说明原因
- [x] 5.3 运行 `openspec validate --all`，确保 delta spec 可在归档时同步到 `openspec/specs/model-resolution/`

## 6. 验证

- [x] 6.1 运行 `cargo check --release --all-targets`，报告告警数并确认相对基线零新增
- [x] 6.2 运行 `cargo test --release --locked`，确认 converter 与 handlers 模块无回归
- [x] 6.3 复核四个成功标准：归一、上下文窗口、adaptive thinking、静态 fallback

## 7. 合规与完成门禁

- [x] 7.1 运行 `spec-compliance-check` 并修复范围、设计、场景、项目规则、验证与文档同步问题
- [x] 7.2 运行 `openspec-verify-change`，产出归档前验证报告
- [x] 7.3 运行 `verification-before-completion`，记录真实命令、告警数、文档同步、`git status --short` 与剩余风险
- [x] 7.4 用户确认后再运行 `openspec-archive-change`
