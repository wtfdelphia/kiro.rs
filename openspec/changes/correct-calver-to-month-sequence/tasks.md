## 1. 基线与实施桥接

- [x] 1.1 运行 `openspec-superpowers-bridge`，记录规格到文件、风险、工具与验证命令的映射
- [x] 1.2 记录基线：`cargo check --release --all-targets` 告警数、门禁测试用例数、全部历史 tag 与其提交日期的对照
- [x] 1.3 确认 `config.json`、`credentials.*` 与 `.codegraph/` 不进入本次变更

## 2. 门禁格式判定修正

- [x] 2.1 将 `CALVER_TAG` 序号段由 `[1-9]\d?` 放宽为 `[1-9]\d*`，保持禁前导零与禁 0
- [x] 2.2 移除 `dt.date()` 日历日校验，改为显式校验月份在 1-12 范围内
- [x] 2.3 更新错误文案：格式提示改为 `vYYYY.MM.MICRO`，非法月份给出独立可诊断信息
- [x] 2.4 确认 `datetime` 导入若不再使用则一并移除，避免新增未使用导入告警

## 3. 门禁测试

- [x] 3.1 替换 `test_rejects_invalid_calendar_date`：`v2026.2.30` 在序号语义下改为**通过**
- [x] 3.2 新增同月多版本正例：`v2026.8.11` 与 `v2026.8.12` 均通过，不因同月或同日被拒
- [x] 3.3 新增反例：月份为 0 或大于 12（`v2026.13.1`、`v2026.0.1`）被拒且文案指向月份
- [x] 3.4 新增反例：序号为 0（`v2026.8.0`）与带前导零（`v2026.08.11`、`v2026.8.011`）被拒
- [x] 3.4b 将 `test_rejects_leading_zero_month_or_day` 改名，使其语义指向 month/micro 而非日期
- [x] 3.5 新增正例：三位以上序号（`v2026.8.100`）通过，验证上界放宽
- [x] 3.6 确认既有附注 tag、Cargo 一致性、main 可达性、tag 指向、0/1/多 tag 解析用例全部保持通过

## 4. 文档与规格同步

- [x] 4.1 更新 README 版本约定：格式改为 `vYYYY.MM.MICRO`，明确第三段为当月序号而非日期，移除同日单版本限制
- [x] 4.2 更新 README 人工镜像发布说明中的 `vYYYY.M.D` 表述
- [x] 4.3 在 README 发版清单补充「查上一个同月 tag 后序号加一」的操作步骤
- [x] 4.4 在 `docs/version-governance-optimization-design.md` 增补修订记录，保留原判断并说明纠正依据（29 个历史 tag 对照、calver.org 与 Twisted 先例）
- [x] 4.4b 同步 `docs/release-version-governance-remaining-verification.md`：修正「同一自然日只能有一个正式版本」等过期表述，并将红路径示例 tag 从 `v2026.8.11` 换号（该号现为本 change 的绿路径目标版本）
- [x] 4.5 核对 AGENTS.md 无需修改并在最终报告说明原因
- [x] 4.6 运行 `openspec validate --all`

## 5. 本地验证

- [x] 5.1 运行 `cargo check --release --all-targets`，报告告警数并确认相对基线零新增
- [x] 5.2 运行门禁测试模块，确认全绿且用例数不低于基线
- [x] 5.3 用 `check_release_version.py validate` 对 `v2026.8.11` 本地演练，确认通过
- [x] 5.4 确认 `version-gate.yaml` 与两条 caller workflow 未被修改（`git diff` 为空）

## 6. CI 红绿路径重新取证

- [x] 6.1 由维护者创建 Cargo 失配的临时附注 tag，验证两条 workflow 均被 version gate 拦截且产物 job skipped，随后删除远端临时 tag
- [x] 6.2 将 `Cargo.toml` 与 `Cargo.lock` 更新为 `2026.8.11` 并经 PR 合入 main
- [x] 6.3 由维护者推送 `v2026.8.11` 正式 tag，验证绿路径产物链完整，Release 资产名与 GHCR 镜像 tag 均为 `v2026.8.11`
- [x] 6.4 确认该正式版本包含 Claude Opus 5 支持（提交 `9d4bdba` 可从 tag 到达）

## 7. 合规与完成门禁

- [x] 7.1 运行 `spec-compliance-check` 并修复范围、设计、场景、项目规则、验证与文档同步问题
- [x] 7.2 运行 `openspec-verify-change`，产出归档前验证报告
- [x] 7.3 运行 `verification-before-completion`，记录真实命令、告警数、文档同步、`git status --short` 与剩余风险
- [ ] 7.4 用户确认后再运行 `openspec-archive-change`
- [ ] 7.5 归档同步时手动修正主规格 `## Purpose` 段中的 `vYYYY.M.D` 表述（delta 只含 Requirements 段，不覆盖 Purpose，易漏）
