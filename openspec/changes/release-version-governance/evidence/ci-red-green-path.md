# CI 红/绿路径验证（任务 7.5 / 7.6）

Change：`release-version-governance`
日期：2026-08-10
仓库：`wtfdelphia/kiro.rs`

## 前置：实现合入 main

7.5/7.6 都要求正式 tag 从 `origin/main` 可达，且 `version-gate.yaml` 必须已存在于 `main`。
实现此前只在 `dev-test`，因此先以普通 merge（`--no-ff`）经 PR 合入，无强推、无 reset、无分支删除。

| 项 | 值 |
| --- | --- |
| PR | https://github.com/wtfdelphia/kiro.rs/pull/6 |
| 合并方式 | merge commit（`gh pr merge --merge`），分支保留 |
| 合并后 main | `bf1f0b3` |
| main 上 workflow | `build.yaml`、`docker-build.yaml`、`version-gate.yaml`、`warning-gate.yaml`、`build-dev-release.yaml` |

PR 合并本身触发了 `main` push 的两条正式 workflow，实测证实稳定分支触发器已从 `master` 迁至 `main`。

## 第一轮红路径：暴露门禁缺陷（v2026.8.11）

临时附注 tag `v2026.8.11` 指向 `bf1f0b3`，Cargo 版本为 `2026.8.10`，唯一违规点应是 Cargo 一致性。

| Workflow | Run | 结论 |
| --- | --- | --- |
| Build Artifacts | https://github.com/wtfdelphia/kiro.rs/actions/runs/31362558798 | `pre-check` success、`version-gate` **failure**、`warning-gate` success、`build` **skipped**、`release` **skipped** |
| Build and Push Docker Images | https://github.com/wtfdelphia/kiro.rs/actions/runs/31362558781 | `pre-check` success、`version-gate` **failure**、`warning-gate` success、`build` **skipped**、`manifest` **skipped** |

拦截行为与 skipped 传播完全符合规格，但**失败原因不符合预期**：

```
::error::release tag 'v2026.8.11' must be an annotated tag
```

而本地对同一 tag 执行同一命令报的是预期的 Cargo 失配：

```
::error::Cargo.toml version '2026.8.10' does not match release tag 'v2026.8.11';
set [package].version to '2026.8.11', update Cargo.lock, commit, and recreate the tag
```

远端 tag 对象类型经 API 确认为 `tag`（附注）：
`gh api repos/wtfdelphia/kiro.rs/git/ref/tags/v2026.8.11 --jq '.object'` →
`{"sha":"60e3171...","type":"tag"}`。

### 根因

`version-gate` job 日志显示 `actions/checkout@v5` 在 tag 已存在时执行：

```
git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules \
  origin +bf1f0b3a6ffcd803f48330131f4824d24673cb05:refs/tags/v2026.8.11
```

该 refspec 把 **commit SHA** 覆盖写入本地 `refs/tags/v2026.8.11`，于是
`git cat-file -t refs/tags/<tag>` 返回 `commit`，附注 tag 检查对**任何**正式 tag 都会误判。
本地最小复现已确认：fetch tag ref 后类型为 `tag`，按上述 refspec 再 fetch 一次即变为 `commit`。

影响面：这是先于 Cargo 一致性的判定，因此绿路径（7.5）在修复前必定被误拦。第一轮红路径
虽然「拦住了」，但拦截理由错误，不能作为 Cargo 一致性判定的有效证据。

### 修复

门禁改为从远端权威判定 tag 类型，不再依赖被 checkout 改写的本地 ref：

- `scripts/check_release_version.py` 新增 `tag_object_type()`，通过 `git ls-remote` 的
  peeled `^{}` 行区分附注与轻量 tag；新增 `--remote` 参数；缺省仍走本地 `cat-file`，保持向后兼容
- 必须使用 glob refspec（`refs/tags/<tag>*`）：git 在 refspec 精确命名某个 ref 时会抑制
  peeled 行，而该行是唯一的判定信号
- `rev-parse` 由 `^{}` 改为 `^{commit}`，使解引用对两种 ref 形态都稳定
- `version-gate.yaml` 传入 `--remote origin`
- 新增 4+2 个回归用例：checkout 改写后仍接受合法附注 tag、改写后正确报 Cargo 失配、远端轻量
  tag 被拒、tag 不在远端被拒，以及前缀共享（`v2026.8.1*` 也匹配 `v2026.8.10`）的两个方向

`python -m unittest` 三个模块：**40 passed**（原 23 + 新增回归与既有覆盖）。

## 第二轮红路径：修复后重跑（v2026.8.11）

删除第一轮临时 tag 后，在修复后的 main（`55b64a3`）上重建同名附注 tag，Cargo 仍为 `2026.8.10`。

| Workflow | Run | 结论 |
| --- | --- | --- |
| Build Artifacts | https://github.com/wtfdelphia/kiro.rs/actions/runs/31363555851 | `version-gate` **failure**、`warning-gate` success、`build` **skipped**、`release` **skipped** |
| Build and Push Docker Images | https://github.com/wtfdelphia/kiro.rs/actions/runs/31363555870 | `version-gate` **failure**、`warning-gate` success、`build` **skipped**、`manifest` **skipped** |

两条 workflow 的 annotation 均为预期原文：

```
::error::Cargo.toml version '2026.8.10' does not match release tag 'v2026.8.11';
set [package].version to '2026.8.11', update Cargo.lock, commit, and recreate the tag
```

### 无发布副作用确认

| 检查 | 结果 |
| --- | --- |
| GitHub Releases | 无 `v2026.8.11`；最新正式 Release 仍为 `v2026.8.4` |
| GHCR tag 列表（`gh api users/wtfdelphia/packages/container/kiro-rs/versions`） | 无任何 `v2026.8.11*`；最新正式镜像仍为 `v2026.8.4` 与 `latest` |

### 任务 7.7 运行时证据（gate 并行）

取自 run `31363555851` 的 job 起止时间：

| Job | started | completed |
| --- | --- | --- |
| `pre-check` | 06:51:30Z | 06:51:35Z |
| `version-gate` | 06:51:37Z | 06:51:43Z |
| `warning-gate` | 06:51:37Z | 06:52:06Z |

两个 gate 同秒启动；version gate 6 秒内失败，warning gate 持续 29 秒。version gate 不等待
warning gate 完成，实测证实二者正交并行。

## 任务 7.4：非正式构建可追溯（实测）

`dev` 推送至 `55b64a3` 后：

| Workflow | Run | version gate |
| --- | --- | --- |
| Build Artifacts（`build.yaml`） | https://github.com/wtfdelphia/kiro.rs/actions/runs/31364313666 | 存在但为 **success**（`enforce=false` 非强制通道；不带 `if`，避免 skipped 传播误伤分支产物） |
| Build Dev Release（`build-dev-release.yaml`） | https://github.com/wtfdelphia/kiro.rs/actions/runs/31364313707 | **完全不存在** version gate |

两条均 success，7 条产物腿全部通过。`dev-latest` Release 元数据：

```
name: Dev rolling build (55b64a3)
tag: dev-latest | prerelease: true
target: 55b64a34acd083d1fbd45f7f4b0b1a205e3d17a1
body: Commit: 55b64a34acd083d1fbd45f7f4b0b1a205e3d17a1 / Short SHA: 55b64a3 / Workflow: Build Dev Release
      "This prerelease does not replace the repository Latest stable release."
```

标题含短 SHA、正文含完整 commit 与 workflow，标记为 prerelease，满足「可追溯且不冒充正式版本」。

## 分支收敛：dev 推送后 PR 合入 main 与 master

按维护者要求，最终形态为 `dev` 推送、`main` 与 `master` 均经 PR 合入。

| 步骤 | 内容 |
| --- | --- |
| PR #6 | `codex/release-version-governance-main` → `main`，普通 merge，实现落地 |
| PR #7 | `codex/release-version-governance-main` → `main`，普通 merge，version gate 附注 tag 判定修复 |
| `dev` | fast-forward 至 `55b64a3`（与 `main` 同点），无历史改写 |
| PR #8 | `dev` → `master`，普通 merge，master 合并后为 `671fadd` |
| PR #9 | `dev` → `main`，普通 merge，证据回填，main 合并后为 `674c2cc` |
| PR #10 | `dev` → `master`，普通 merge，同批证据同步，master 合并后为 `0e1071a` |

收敛后核验：

| 检查 | 结果 |
| --- | --- |
| `git diff --stat origin/main origin/master` | 无输出（树内容一致） |
| `git diff --quiet origin/dev origin/main` | exit 0（一致） |
| `git diff --quiet origin/main origin/master` | exit 0（树内容一致） |
| 三分支 `Cargo.toml` 版本 | 均为 `2026.8.10` |
| `master` 上 workflow | 含 `version-gate.yaml`，与 `main` 一致 |

### 合并拓扑说明

`dev` 分别向 `main` 与 `master` 开 PR，每次合并各自生成一个 merge commit，因此两条分支
互有「独有提交」，但双向**非合并**提交差异均为 0，树内容经 `git diff --quiet` 证实完全一致。

由此产生一条发布纪律：正式 tag MUST 打在 `main` 上。门禁校验 `origin/main` 可达性，而
`master` 的 merge commit 不从 `main` 可达；在 `master` HEAD 上打正式 tag 会被门禁正确拒绝。
这与 D4「main 是唯一稳定发版落点」一致。

全程无强推、无 reset、无分支删除。

## 临时 tag 清理确认

两轮红路径的临时 tag 均已删除：

| 轮次 | 操作 | 确认 |
| --- | --- | --- |
| 第一轮（`60e3171`） | `git push origin :refs/tags/v2026.8.11` + `git tag -d` | 远端 `- [deleted] v2026.8.11` |
| 第二轮（`a8bc27c`） | 同上 | `git ls-remote --tags origin \| rg 2026.8.11` 无输出 |

失败的 Actions run 记录保留在历史中，它们正是红路径证据本身。

## 任务 7.5：正式发布绿路径（未执行）

**状态：BLOCKED，待维护者授权。**

正式附注 tag `v2026.8.10` 已在本地创建并指向 `55b64a3`（`git cat-file -t` 为 `tag`），
Cargo 版本 `2026.8.10` 与当日 CalVer 一致，前置条件齐备。

推送该 tag 会创建真实的 GitHub Release 与 GHCR 正式镜像（`v2026.8.10` 与 `latest`），
属不可逆的高影响外部副作用，实施代理未获得该具体动作的授权，因此停在推送前。

待授权后需补充的证据：两条 workflow 的绿路径 run URL、Release 资产名、
`docker image inspect ghcr.io/wtfdelphia/kiro-rs:v2026.8.10` 的 OCI version label。

需注意：`main` 与 `master` 现已同点，`master` 上也存在 `v*` tag 触发器；正式 tag 指向的
提交同时可从两条分支到达，门禁的 `origin/main` 可达性检查不受影响。
