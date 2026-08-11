# Spec Compliance Report

Change：`correct-calver-to-month-sequence`
日期：2026-08-11
范围：任务 1.1-6.4 实现后、归档前审查
审查基线：`8d6eeca`（本 change 前的 main）→ `1aa5b53`（当前 dev）
总体状态：**PASS**

## 六维审查

| 维度 | 状态 | 依据 |
| --- | --- | --- |
| Scope | PASS | 代码改动仅 `scripts/check_release_version.py`(+22/-9)、`scripts/tests/test_check_release_version.py`(+45/-6)；另有 `Cargo.toml`/`Cargo.lock` 版本号各 1 行、`README.md`、两份 `docs/`。**`.github/` 零改动**（`git diff --stat 8d6eeca..HEAD -- .github/` 为空），非目标未被触碰：无 Rust 源码改动、无 Docker、无资产命名、无 OCI label、未引入自动序号推导、未转 SemVer、未加第四段 |
| Design | PASS | D1 序号段 `[1-9]\d*` 且显式补 `1<=month<=12`；D2 门禁不比较历史 tag，唯一性交给 git；D3 历史 tag 未改写；D4 附注 tag、Cargo 一致性、tag 指向、main 可达性、无修订后缀、不补零全部保留 |
| Scenarios | PASS | MODIFIED 的 5 个与 ADDED 的 2 个 Scenario 全部有实现与测试证据，见下方映射表 |
| Project Rules | PASS | 走完整 OpenSpec 流程；`cargo check --release --all-targets` 零告警；bridge plan 含 CodeGraph 与 rg 补盲；红/绿 CI 双路径均有 run 证据（AGENTS.md 对 CI 门禁类变更的硬性要求）；无真实凭据或 `.codegraph/` 进入提交 |
| Verification | PASS | 全部命令本会话真实运行。门禁三模块 48 passed（基线 40）；红路径两条 run failure 且产物 skipped；绿路径两条 run 全 success 且产物齐备。无 SKIPPED 项 |
| README/AGENTS Sync | PASS | README 已改（格式、序号语义、同日限制移除、人工发布表述、发版清单序号查法）；两份 docs 已同步并保留原判断标注；AGENTS.md 与顶层 `spec/`、`openspec/project.md` 经 rg 确认不含版本格式约定，无需修改 |

## Scenario 到证据映射

| Requirement | Scenario | 证据 |
| --- | --- | --- |
| 唯一且一致的发布身份（MODIFIED） | 合法正式版本通过身份校验 | 绿路径 run `31450853797`/`31450853794` 的 `version-gate` success；本地演练 `release identity valid: v2026.8.11` |
| 同上 | 同月第二个正式版本被接受 | `test_accepts_multiple_releases_in_same_month`（`v2026.8.11` 与 `v2026.8.12` 均通过） |
| 同上 | 序号无需对应日历日 | `test_accepts_sequence_that_is_not_a_calendar_day`（`v2026.2.30` 由反例转正例）；红路径 `v2026.8.900` 三位序号未被格式判定拒绝 |
| 同上 | Cargo 与 tag 漂移时拒绝发布 | 红路径两条 run annotation：`Cargo.toml version '2026.8.10' does not match release tag 'v2026.8.900'`；产物 job 全 skipped |
| 同上 | 非法 CalVer 或轻量 tag 被拒绝 | `test_rejects_month_above_twelve`、`test_rejects_zero_month_or_zero_sequence`、`test_rejects_leading_zero_month_or_micro`、`test_rejects_revision_suffix`、`test_rejects_lightweight_tag` |
| 发布序号由维护者显式决定（ADDED） | 跳号不影响门禁 | 门禁代码不含历史 tag 比较逻辑；`test_accepts_sequence_above_two_digits`（`v2026.8.100` 直接通过，与上一序号不连续） |
| 同上 | 门禁不因缺少历史 tag 而失败 | `validate_release` 只用 `ls-remote` 查单个 tag 的 peeled 行与 `merge-base` 判可达；红/绿路径在 `fetch-tags: false` 的 checkout 环境下均正确判定 |

## 发现项

1. **红路径临时 tag 刻意使用三位序号**：`v2026.8.900` 在旧门禁下会被 `[1-9]\d?` 以格式错误拒绝。实际 annotation 报 Cargo 失配，据此单次实验同时证明「上界解除生效」与「一致性判定未被破坏」。这是本 change 最关键的一条证据设计。
2. **执行红路径前发现前提缺失**：`origin/main` 当时仍是旧门禁（`dt.date` 仍在），若直接推 tag 会被旧正则拒掉并掩盖真正要测的判定。故先经 PR #15 合入修正，核实 `dt.date` 消失后才取证。已记入 `evidence/ci-red-path.md`。
3. **rg 补盲新增两处文档同步**（超出原 tasks）：`docs/release-version-governance-remaining-verification.md` 的「改期就改日期」表述，以及它用 `v2026.8.11` 作红路径示例——该号已成为本 change 的绿路径目标版本，会造成指引冲突，已换为 `v2026.8.900`。对应 tasks 3.4b / 4.4b。
4. **主规格 `## Purpose` 段不在 delta 覆盖范围**：delta 只含 `## Requirements`，而 Purpose 段亦有 `vYYYY.M.D` 表述。已新增 task 7.5 钉住，归档同步时须手动修正，否则会遗留旧格式。
5. **跨天验证了方案稳定性**（非缺陷）：改版发生在 08-10，发布在 08-11。序号语义下版本号不受日期变化影响；日历日语义下跨天会使已定版本号失效。
6. **`codegraph sync` 未执行**：索引有 pending changes。本 change 不改 Rust 源码，改动点已由 rg 精确定位，不 sync 以免把索引变更混入提交。非阻塞。

## 剩余风险

- 历史区段序号存在语义空洞：`v2026.7.27` 起若干 tag 的第三段源于日期巧合，7 月并未真的发布 31 次。按 D3 不追溯改写，以保证已发布镜像与二进制引用稳定。
- 本机无 actionlint：workflow 未做 Actions 专用 lint。但本 change `.github/` 零改动，该风险不新增。
- 序号跳号不被门禁拦截（D2 有意设计）。维护者误填序号只会造成号段不连续，不影响单调性与唯一性。
- 未归档（task 7.4）；主规格 Purpose 段待归档时同步（task 7.5）。
