# Tasks: desktop-credentials-view

## 1. CoreHandle 与装配接线

- [x] 1.1 `core/mod.rs`：`CoreHandle { service: Arc<AdminService>, rt }` +
      `exec`（tokio spawn + oneshot 回传）
- [x] 1.2 `bridge.rs`：`CoreEvent::Bootstrapped` 携带 `CoreHandle`，
      derive 去掉 `PartialEq`/`Eq`，往返测试改逐字段断言
- [x] 1.3 `main.rs`：bootstrap 成功后构造桌面 `AdminService`
      （`new_with_runtime`，endpoint_names + kiro_provider），打包进事件
- [x] 1.4 `root.rs`：`AppView` 持 `Option<CoreHandle>`，凭据视图按
      handle 就绪与否切换加载态/数据态

## 2. 凭据表格

- [x] 2.1 `credentials/table.rs`：`CredentialsDelegate`（TableDelegate
      必填四方法），列布局见 design D4
- [x] 2.2 `credentials/mod.rs`：`CredentialsView` 实体，持快照 + 查询
      参数；进入时同步查 `query_credentials` + `get_credential_facets`
- [x] 2.3 筛选栏（认证方式/禁用/订阅/邮箱）+ 分页控件，变更即重查
- [x] 2.4 行内操作按钮接线：启停、优先级、删除、测试、余额、刷新模型
      （全部经 `CoreHandle::exec`，结果通知 + 重拉）

## 3. 对话框

- [x] 3.1 添加凭据（social / api_key / idc 三表单，校验见 design D6）
- [x] 3.2 批量导入（JSON 数组粘贴，本地预解析后
      `import_credentials_batch`，结果汇总进通知）
- [x] 3.3 KAM 导入（原文粘贴，`import_kam_document`）
- [x] 3.4 删除确认（AlertDialog::confirm）

## 4. 在线登录

- [x] 4.1 Builder ID：start → 开浏览器（`open`）→ 按 `interval` 轮询
      到完成/过期；轮询任务独立于对话框生命周期
- [x] 4.2 IAM SSO：startUrl → 开浏览器 → 粘贴回调 → complete；
      浏览器拉起失败保留 URL 供复制

## 5. 依赖与测试

- [x] 5.1 `Cargo.toml`：`open`（运行时）+ `gpui`（dev-dep，
      `test-support`，版本钉锁内）
- [x] 5.2 headless 表格测试：行数/列数/单元格与快照一致
      （真实 AdminService + tempdir SqliteStore）
- [x] 5.3 写操作测试：启停/优先级/删除经 exec 走真实服务，重查断言；
      删除后 SQLite 行与钥匙串条目同步消失
- [x] 5.4 表单校验测试：必填 + `ksk_` 前缀（纯函数）

## 6. 验收

- [x] 6.1 `desktop/` 内 `RUSTFLAGS="-D warnings" cargo check --release
      --all-targets --locked` 零告警
- [x] 6.2 `desktop/` 内 `cargo test --release` 全绿
- [x] 6.3 Xvfb 冒烟：装配 + 凭据视图数据加载（日志链 + 窗口存活）
- [x] 6.4 根仓零改动验证：根侧 `cargo check` 告警数不变；
      `openspec validate desktop-credentials-view` 通过；
      `git status` 无敏感文件
- [x] 6.5 手动全流程留痕：测试/余额/SSO 上游交互（本环境无网络凭据，
      记入证据文件说明剩余风险）
