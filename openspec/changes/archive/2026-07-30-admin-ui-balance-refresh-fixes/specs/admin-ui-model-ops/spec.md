## MODIFIED Requirements

### Requirement: 凭据卡片提供余额强制刷新入口

Each credential card MUST expose a primary control to refresh balance/usage for that credential with cache bypass (force), complementary to the dashboard batch「查询信息」action. The legacy「重置失败」control MAY remain but MUST NOT be the only/primary recovery path when balance refresh is available.

The card MUST render balance/usage from a single source supplied by its parent. A card MUST NOT keep a private copy of balance state that shadows later parent updates: because the balance payload carries no timestamp, a card holding two sources cannot determine which is newer and will display stale data for as long as it stays mounted. A card's own force refresh MUST propagate its result to the parent rather than storing it locally.

#### Scenario: 单卡刷新余额

- **WHEN** 操作者点击「刷新余额」（或等价）
- **THEN** UI 以 force 方式请求该凭据 balance/usage 并更新卡片展示的订阅/剩余信息

#### Scenario: 与批量查询信息互补

- **WHEN** 操作者使用顶栏「查询信息」
- **THEN** 行为仍为当前页批量 balance 查询；与单卡 force 刷新可并存，不互相删除

#### Scenario: 重置失败降级

- **WHEN** 凭据 failureCount 与 refreshFailureCount 均为 0 且未因失败禁用
- **THEN** 「重置失败/恢复」类入口可禁用或弱化展示，且主路径不依赖该按钮完成余额更新

#### Scenario: 单卡刷新后批量结果仍可见

- **GIVEN** 操作者已对某张卡片执行单卡 force 刷新，且该卡片未被卸载
- **WHEN** 随后的批量余额查询为该凭据取得新数据
- **THEN** 卡片 MUST 展示批量查询取得的新数据
- **AND** MUST NOT 因卡片内部保留了先前单卡刷新的副本而继续展示旧值

#### Scenario: 单卡刷新结果回流父级

- **WHEN** 单卡 force 刷新成功
- **THEN** 结果 MUST 被写入父级共享的余额数据源，使其他依赖该数据源的展示保持一致
