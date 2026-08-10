## MODIFIED Requirements

### Requirement: 正式版本必须具有唯一且一致的发布身份

项目 MUST 以 `Cargo.toml [package].version` 作为源码版本声明，以匹配的附注 git tag 作为正式发布
身份。正式 tag MUST 严格使用 `vYYYY.MM.MICRO`，其中 `YYYY` 为四位年份，`MM` 为 1 至 12 的月份，
`MICRO` 为该年月内的发布序号（从 1 开始）。月份与序号 MUST NOT 补零，去掉 `v` 后 MUST 与 Cargo
版本完全一致。正式版本 MUST NOT 使用修订后缀。

第三段是**当月发布序号，不是日历日**。同一年月内 MUST 允许发布多个正式版本，其序号 MUST 严格
递增且 MUST NOT 复用。因此同一自然日 MAY 发布多个正式版本。

> 纠正说明：本要求早期版本曾将第三段定义为日历日（`vYYYY.M.D`）并禁止同日发布第二个正式版。该
> 定义源于只抽样最近 6 个 tag 的误判：项目全部历史 tag（含 `2025.12.1`–`2025.12.7` 在 4 天内发布、
> `2026.2.1`–`2026.2.3` 同日发布）表明原约定始终是当月序号。`v2026.7.27` 起第三段与日期重合属
> 一天一发的巧合。历史 tag MUST NOT 被追溯改写。

#### Scenario: 合法正式版本通过身份校验

- **WHEN** 当前提交具有唯一附注 tag `v2026.8.11`，Cargo 版本为 `2026.8.11`，且提交可从稳定发布分支到达
- **THEN** 版本身份门禁 MUST 通过

#### Scenario: 同月第二个正式版本被接受

- **WHEN** 同一年月内已存在正式版本 `v2026.8.11`，新提交带附注 tag `v2026.8.12` 且 Cargo 版本为 `2026.8.12`
- **THEN** 版本身份门禁 MUST 通过
- **AND** MUST NOT 因两者属于同一自然日或同一月份而拒绝

#### Scenario: 序号无需对应日历日

- **WHEN** 正式 tag 的第三段不是有效日历日（如 `v2026.2.30`）但为合法序号，且其余身份检查通过
- **THEN** 版本身份门禁 MUST 通过
- **AND** MUST NOT 报告「非法日历日期」

#### Scenario: Cargo 与 tag 漂移时拒绝发布

- **WHEN** 正式 tag 去掉 `v` 后与 Cargo 版本不一致
- **THEN** 版本身份门禁 MUST 失败并输出具体修复指引
- **AND** 发布产物构建、Release、镜像与 manifest MUST NOT 启动或创建

#### Scenario: 非法 CalVer 或轻量 tag 被拒绝

- **WHEN** tag 含修订后缀、月份或序号补零、月份超出 1-12、序号为 0，或不是附注 tag
- **THEN** 版本身份门禁 MUST 在构建前失败

## ADDED Requirements

### Requirement: 发布序号必须由维护者显式决定

版本身份门禁 MUST 只校验单个 tag 自身的合法性与其提交的 `origin/main` 可达性，MUST NOT 依据远端
历史 tag 列表推导或强制「序号等于上一个序号加一」。序号的选择 MUST 由维护者在创建 tag 时显式决定。

跳号 MUST NOT 被视为错误。同名 tag 的唯一性由 git 保证。

#### Scenario: 跳号不影响门禁

- **WHEN** 同月上一个正式版本为 `v2026.8.11`，新 tag 为 `v2026.8.15` 且其余身份检查通过
- **THEN** 版本身份门禁 MUST 通过

#### Scenario: 门禁不因缺少历史 tag 而失败

- **WHEN** 门禁运行环境未获取完整历史 tag 列表
- **THEN** 序号合法性判定 MUST NOT 受影响
- **AND** 门禁 MUST NOT 因无法比较历史序号而拒绝合法 tag
