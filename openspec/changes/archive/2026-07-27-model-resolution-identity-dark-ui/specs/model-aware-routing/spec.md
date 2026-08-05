## MODIFIED Requirements

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
