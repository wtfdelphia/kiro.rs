## Why

`release-version-governance` 把正式版本格式定义为 `vYYYY.M.D`（第三段=日历日），并额外禁止修订
后缀，因此同一自然日 MUST NOT 发布第二个正式版。该定义基于一次抽样误判：设计文档只核对了最近
6 个 tag（`v2026.7.27` 起），发现第三段恰好等于当日日期，便认定项目约定为日期版本。

复核全部 29 个历史 tag 后，该结论不成立。项目原有约定是 `YYYY.MM.MICRO`（第三段=当月发布序号）：

| tag | 指向提交日期 | 说明 |
| --- | --- | --- |
| `2025.12.1` … `2025.12.7` | 2025-12-28 至 12-31 | 4 天内发 7 个版本，第三段显然不是日期 |
| `2026.1.2`、`2026.1.3` | 均为 2026-01-07 | 同日两发 |
| `2026.2.1`、`2026.2.2`、`2026.2.3` | 均为 2026-02-06 | 同日三发 |
| `v2026.1.4`、`v2026.1.5` | 均为 2026-01-13 | 同日两发 |

`v2026.7.27` 之后第三段与日期重合，是当时一天一发的巧合，而非约定变更。

后果是能力回退：原约定天然支持同日多次发布，现行门禁将其判为违规。2026-08-10 已发布
`v2026.8.10`，同日完成的 Claude Opus 5 支持（`9d4bdba`，晚于该 tag 指向的 `674c2cc`）因此无法
发布正式版——这不是设计取舍，是误判引入的约束。

外部规范支持恢复原约定。calver.org 明确收录 `YYYY.MM.MICRO` 三段式，Twisted 自 2002 年沿用至今
并扩散到 Klein、Treq、PyOpenSSL；其弃用 SemVer 的理由（组件众多、各自独立弃用与破坏兼容）与本
项目一致。同一规范指出「four-numeric-segment versions are discouraged」，故 `vYYYY.M.D.N` 不在
备选内；即便是纯日期方案代表 youtube-dl，也保留了 micro 段作为技术场景的逃生舱。

SemVer 不适用：semver.org 的前提是识别 public API 并向依赖方传递兼容性信号，而本项目是
Anthropic/OpenAI 兼容代理，用户以 Docker 镜像或二进制运行，无下游代码依赖，该信号没有接收者。

## What Changes

- 将正式版本格式由 `vYYYY.M.D`（日历日）修正为 `vYYYY.MM.MICRO`（当月发布序号），月份与序号均不
  补零，保持三段式与 `v` 前缀不变。
- 移除「第三段必须是有效日历日期」校验；改为校验序号为不小于 1 的整数、月份在 1-12 范围内。
- 移除「同一自然日只允许一个正式版」约束。同月可发布多个正式版，序号递增。
- 保留全部既有身份约束：附注 tag、Cargo 一致性、`origin/main` 可达性、无修订后缀、不补零、
  tag 指向发布提交。
- 新增「同月序号必须单调递增且不复用」约束，替代原先由日期天然提供的唯一性保证。
- 同步 README、`docs/version-governance-optimization-design.md` 与长期规格。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `release-version-governance`: 修正正式版本身份的格式定义与同期多版本规则；其余要求（main 落点、
  人工发布约束、运行时与镜像可观测性、非正式构建追溯、MSRV、admin-ui 策略）不变。

## Impact

- 门禁：`scripts/check_release_version.py`（格式正则、日历日校验替换为序号校验、错误文案）。
- 测试：`scripts/tests/test_check_release_version.py`（`test_rejects_invalid_calendar_date` 语义
  失效需替换；补同月多版本正例与序号非法反例）。
- 文档：`README.md` 版本约定与人工发布说明；
  `docs/version-governance-optimization-design.md` 增补修订记录，保留原判断与纠正过程。
- 规格：`release-version-governance` delta；归档后同步到 `openspec/specs/`。
- 不影响：`version-gate.yaml` 接线、`build.yaml` / `docker-build.yaml` 的 needs 依赖、warning gate、
  Dockerfile OCI label、`Cargo.toml` 当前版本值、任何 Rust 源码。
- 历史 tag 不改写。`v2026.7.27` 起若干 tag 的第三段与日期重合的痕迹保留，仅在文档说明其为巧合。

## Assumptions

- 下一个正式版为 `v2026.8.11`，语义是「2026 年 8 月第 11 次发布」，而非 8 月 11 日。该值与既有
  `v2026.8.10` 连续，且经核实远端不存在。
- 8 月已发布版本的序号最大值为 10（`v2026.8.4` 与 `v2026.8.10` 中的 10 视为序号）。第三段与日期
  重合的历史 tag 在序号语义下仍单调递增，无需追溯修正。

## Success Criteria

- `v2026.8.11` 在 Cargo 版本为 `2026.8.11` 时通过门禁（当前会被日历日语义误判为「8 月 11 日」而
  仍然合法，但需确认新校验不依赖日期有效性）。
- `v2026.2.30` 这类原先因「非法日历日」被拒的 tag，在新规则下按序号 30 合法通过。
- 同月连续两个正式版（如 `v2026.8.11` 与 `v2026.8.12`）均可发布，不触发同日限制。
- 序号为 0 或带前导零（`v2026.8.0`、`v2026.08.11`、`v2026.8.011`）仍被拒。
- `cargo check --release --all-targets` 零新增告警；门禁测试全绿；红/绿 CI 路径重新取证。
