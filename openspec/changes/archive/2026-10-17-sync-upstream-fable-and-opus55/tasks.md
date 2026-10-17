## 1. 基线与实施桥接

- [x] 1.1 运行 `openspec-superpowers-bridge`，记录规格到文件、风险、工具与验证命令的映射
- [x] 1.2 记录实现前基线：`cargo check --release --all-targets` 告警数、
  `cargo test` 通过数
- [x] 1.3 确认 `config.json`、`credentials.*` 与 `.codegraph/` 不进入本次变更

## 2. 归一分支移植

- [x] 2.1 在 `normalize_claude_model`（`src/anthropic/converter.rs`）新增 fable 分支：置于
  sonnet/opus 之前，无条件映射 `claude-fable-5.1`，与上游 `fcb6e7a` 语义一致；
  验证：`normalize_claude_model("claude-fable-5-1")` 与 `("claude-fable-5.1")` 均返回
  `Some("claude-fable-5.1")`
- [x] 2.2 opus 分支新增 `opus-5.5`/`opus-5-5` 判定，置于 `opus-5` 之前（与上游
  `9741858` 语义一致，避免 contains 误吞）；验证：`claude-opus-5-5`、`claude-opus-5.5`、
  `claude-opus-5.5-20260922` 归一为 `claude-opus-5.5`，`claude-opus-5` 仍归一 `claude-opus-5`

## 3. 上下文窗口与 thinking

- [x] 3.1 `get_context_window_size` 白名单加入 `claude-fable-5.1` 与 `claude-opus-5.5`；
  验证：两者返回 1_000_000，既有 1M 模型不回归
- [x] 3.2 `override_thinking_from_model_name`（`src/anthropic/handlers.rs`）adaptive
  thinking 判定加入 `fable`（opus-5.5 已被既有 `opus-5` 包含匹配覆盖，不加）；验证：
  fable-thinking 请求 thinking 类型为 adaptive 且附 high effort output config

## 4. tool_result 图片修复

- [x] 4.1 `extract_tool_result_content` 返回值改为 `(String, Vec<KiroImage>)`，解析
  `type=image` 块的 base64 数据，复用 `get_image_format` 与 `KiroImage::from_base64`
  （与上游 `fcb6e7a` 语义一致）；验证：构造含 text+image 块的 tool_result，转换后
  images 含该图片
- [x] 4.2 `process_message_content` 的 tool_result 分支合并提取的图片到既有 `images`
  返回；验证：回归测试通过，既有图片提取路径不受影响

## 5. 静态模型列表与文档

- [x] 5.1 `static_fallback_models` 新增 `claude-opus-5-5`、`claude-opus-5-5-thinking`、
  `claude-fable-5-1`、`claude-fable-5-1-thinking` 四个条目，展示 id 用连字符形式与
  上游及既有条目风格一致（归一输出仍是点号形式）；`created` 沿用上游取值
  （opus-5.5: 1790035200，fable: 1756684800）；验证：
  `static_fallback_models_has_core_ids` 与 `static_fallback_models_all_mappable`
  扩展断言后通过
- [x] 5.2 `README.md` 模型映射表补 `*fable*` 一行（置于表首，与上游一致）与
  `*opus-5-5*` / `*opus-5.5*` 一行（置于 `*opus-5*` 之前，与归一判定顺序一致）；
  验证：表格行与代码分支一一对应

## 6. 测试

- [x] 6.1 新增归一测试：fable/opus-5.5 正反向断言（含连字符、点号、thinking、日期后缀
  拼写，及 opus-5 不被 5.5 截获、opus-4-5 不被 5/5.5 截获），对齐上游 `9741858`/
  `fcb6e7a` 的正向断言；上游 `22f36e6` 的负向断言（`claude-sonnet-4-20250514`、
  `claude-3-5-sonnet-20241022`、`claude-opus-4` 判为不支持）与 dev 宽松兼容分支冲突，
  不移植（见 design 决策 6）；验证：`cargo test --release --locked` 相关用例通过
- [x] 6.2 新增 tool_result 图片提取测试：含 image 块的 tool_result 提取出图片、
  无 image 块时行为不变；验证：测试通过
- [x] 6.3 全量回归：`cargo test --release --locked` 全部通过，记录基线变化

## 7. 收尾验证

- [x] 7.1 `cargo check --release --all-targets` 零新增告警（对照变更基线）
- [x] 7.2 `openspec validate --all` 通过；`git status --short` 确认无凭据与
  `.codegraph/` 误入

## 8. 合规与完成门禁

- [x] 8.1 运行 `spec-compliance-check`，产出六维审查报告并修复发现项
- [x] 8.2 运行 `openspec-verify-change`，产出归档前验证报告
- [x] 8.3 运行 `verification-before-completion`，记录真实命令、告警数、文档同步、
  `git status --short` 与剩余风险
- [x] 8.4 用户确认后再运行 `openspec-archive-change`
