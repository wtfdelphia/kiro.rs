# Capability: model-aware-routing

## Purpose

When acquiring a credential for chat, filter the pool by each credential's cached model set (cold-start optimistic when cache is missing/empty), while composing with existing opus subscription filtering and priority/balanced load-balancing modes.

## Requirements

### Requirement: 按模型集合过滤凭据

When selecting a credential for a chat request, the system MUST skip credentials whose cached model set is non-empty and does not contain the resolved upstream Kiro model id produced by the model-resolution pipeline. For alias-mapped models, comparison MUST use the mapped upstream id; for catalog passthrough models, comparison MUST use the passthrough id. Existing dash/dot equivalence MAY continue to apply when matching Claude-style ids.

#### Scenario: 有缓存且不含目标模型

- **WHEN** 请求解析为上游 model X，凭据 A 的 model set 非空且不含 X，凭据 B 含 X
- **THEN** 选择逻辑跳过 A 并优先在含 X 的可用凭据中选择

#### Scenario: 别名按映射后 id 过滤

- **WHEN** 客户端请求 gpt-4o 且 resolve 为 claude-sonnet-4.5，凭据 model set 仅含 claude-sonnet-4.5
- **THEN** 该凭据可作为候选（在其他可用性条件满足时），不得因 set 不含字面量 gpt-4o 而误杀

#### Scenario: 透传按原 id 过滤

- **WHEN** 客户端请求 gpt-5.6-sol 且 resolve 为 passthrough gpt-5.6-sol，凭据 model set 含 gpt-5.6-sol
- **THEN** 该凭据可作为候选

#### Scenario: 冷启动乐观放行

- **WHEN** 凭据尚无 model set 或 set 为空
- **THEN** 不得仅因缺少模型缓存而跳过该凭据（在其他可用性条件满足时仍可被选中）

### Requirement: 与订阅过滤共存

Model-set filtering MUST compose with existing opus subscription filtering: Free-tier credentials that do not support opus MUST remain ineligible for opus models even if model cache is missing or incomplete.

#### Scenario: Free 账号请求 opus

- **WHEN** 请求模型为 opus 且凭据 supports_opus 为 false
- **THEN** 该凭据不得被选中

### Requirement: 负载均衡模式保持

Priority and balanced selection modes MUST continue to apply among credentials that pass model and availability filters.

#### Scenario: balanced 在候选集内

- **WHEN** load_balancing_mode 为 balanced 且多个凭据均支持目标模型
- **THEN** 在通过过滤的候选集上应用 least-used（或现有 balanced 语义）选择
