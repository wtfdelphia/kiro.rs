# GitHub Actions 使用说明

本文说明本仓库 `.github/workflows` 下现有 CI/CD 配置的触发方式、产物形态，以及如何基于 `master` / `dev` / tag 发布。

## 目录结构

```text
.github/
└── workflows/
    ├── build.yaml               # 多平台二进制构建与 Artifact 上传
    ├── build-dev-release.yaml   # dev 分支构建并自动发 prerelease
    └── docker-build.yaml        # Docker 镜像构建并推送到 GHCR
```

当前 `.github` 只有这两份工作流，没有 Dependabot、Issue 模板、自动 Release 等其他配置。

| 工作流 | 文件 | 作用 |
| --- | --- | --- |
| Build Artifacts | [`.github/workflows/build.yaml`](../.github/workflows/build.yaml) | 编译 admin-ui + Rust，产出多平台 `kiro-rs` 可执行文件 |
| Build and Push Docker Images | [`.github/workflows/docker-build.yaml`](../.github/workflows/docker-build.yaml) | 构建 amd64/arm64 镜像，推送到 GHCR，并创建 multi-arch manifest |

## 共同触发条件

两份工作流的触发条件一致：

```yaml
on:
  push:
    branches:
      - master
      - dev
    tags:
      - 'v*'
  workflow_dispatch:
    inputs:
      version:
        description: 'Version ...'
        required: true
```

> 说明：上面是 `build.yaml` 的触发条件。`build-dev-release.yaml` 只监听 `dev`（以及手动触发），用于滚动 prerelease。

| 触发方式 | 是否触发 | 版本结果 | 说明 |
| --- | --- | --- | --- |
| push 到 `master` | 是 | `beta-<sha 前 6 位>` | 日常 beta 构建 |
| push 到 `dev` | 是（两份相关工作流） | artifact: `dev-<sha6>`；release: 滚动 `dev-latest` | `build.yaml` 出 artifact；`build-dev-release.yaml` 发 prerelease |
| push 到其他分支 | 否 | - | 仅 `master` / `dev` 在 branches 列表中 |
| push `v*` tag | 是 | tag 名本身，如 `v2026.3.1` | 正式发布路径 |
| Actions 手动 Run workflow | 是 | 你填写的 `version` | 可选择任意 branch，包括 `dev` |

### pre-check 防重复逻辑

两份工作流都有 `pre-check`：

- 若事件是 **push 到分支**
- 且当前 commit 已经有 `v*` tag

则跳过本次 beta 构建，避免同一提交同时产出 beta 与正式版。

手动触发和 tag 触发不受此限制。

## 1. Build Artifacts（二进制）

### 做什么

1. 安装 Node.js 22 + pnpm 11
2. 在 `admin-ui` 执行 `pnpm install --frozen-lockfile` 与 `pnpm build`
3. 安装对应 Rust target
4. 执行 `cargo build --release --target <target>`
5. 上传 GitHub Actions Artifact

### 平台矩阵

| 运行器 | Target | 产物名后缀 |
| --- | --- | --- |
| macos-latest | aarch64-apple-darwin | macOS-arm64 |
| macos-latest | x86_64-apple-darwin | macOS-x64 |
| windows-latest | x86_64-pc-windows-msvc | Windows-x64 |
| ubuntu-22.04 | x86_64-unknown-linux-gnu | Linux-x64 |
| ubuntu-22.04-arm | aarch64-unknown-linux-gnu | Linux-arm64 |
| ubuntu-22.04 | x86_64-unknown-linux-musl | Linux-musl-x64 |
| ubuntu-22.04-arm | aarch64-unknown-linux-musl | Linux-musl-arm64 |

补充：

- musl target 会安装 `musl-tools`
- musl target 使用 `cargo build --release --target <target> --no-default-features`
- 上传路径包含：
  - `target/<target>/release/kiro-rs`
  - `target/<target>/release/kiro-rs.exe`

### 产物命名

```text
kiro-rs-<version>-<platform-name>
```

示例：

- `kiro-rs-beta-a1b2c3-Linux-x64`
- `kiro-rs-v2026.3.1-Windows-x64`
- `kiro-rs-2026.3.2-macOS-arm64`（手动填写 version 时）

### 如何获取

1. 打开仓库 GitHub 页面
2. 进入 **Actions**
3. 选择 **Build Artifacts**
4. 打开成功的 run
5. 在页面底部 **Artifacts** 下载对应平台压缩包

### 重要限制

对分支 push（`master`/`dev`）仍主要上传 Artifact；对 **`v*` tag** 会在全部平台构建成功后自动创建正式 GitHub Release，并挂载多平台二进制。

如果 README 中“去 Release 下载二进制”可用，通常还需要：

1. 等 Build Artifacts 成功
2. 下载各平台产物
3. 手工创建 GitHub Release 并挂载文件

或另外增加自动 release workflow。


### 正式版自动 Release（v*）

`build.yaml` 在 push `v*` tag 且构建成功后会：

1. 汇总各平台 staged 二进制
2. 使用 `softprops/action-gh-release` 创建正式 Release
3. `prerelease: false`、`make_latest: true`
4. Release 说明中附带 GHCR 拉取命令：`ghcr.io/<owner>/kiro-rs:<tag>` / `:latest`

因此 tag 构建成功后，二进制会出现在仓库首页 Releases，而不仅是 Actions Artifacts。

## 2. Build and Push Docker Images

### 做什么

1. 分别在 amd64 / arm64 runner 构建镜像
2. 登录 `ghcr.io`
3. 推送单架构 tag
4. 创建并推送 multi-arch manifest

### 镜像仓库

镜像前缀：

```text
ghcr.io/<github.repository_owner>/kiro-rs
```

例如：

- 本仓库：`ghcr.io/wtfdelphia/kiro-rs`
- 包页面：https://github.com/wtfdelphia/kiro.rs/pkgs/container/kiro-rs

### 单架构 tag

```text
ghcr.io/<owner>/kiro-rs:<version>-amd64
ghcr.io/<owner>/kiro-rs:<version>-arm64
```

### multi-arch 别名

| 场景 | 版本 tag | 额外别名 |
| --- | --- | --- |
| push `master` | `beta-<sha>` | `beta` |
| push `v*` tag | tag 名 | `latest` |
| 手动 workflow_dispatch | 你填写的 version | `latest` |

示例：

```bash
docker pull ghcr.io/<owner>/kiro-rs:latest
docker pull ghcr.io/<owner>/kiro-rs:beta
docker pull ghcr.io/<owner>/kiro-rs:v2026.3.1
```

### 与 docker-compose 的关系

仓库根目录 [`docker-compose.yml`](../docker-compose.yml) 默认类似：

```yaml
image: ghcr.io/${IMAGE_OWNER:-wtfdelphia}/kiro-rs:${IMAGE_TAG:-latest}
```

本地可覆盖：

```powershell
$env:IMAGE_OWNER="wtfdelphia"
$env:IMAGE_TAG="v2026.3.1"
docker compose up -d
```

如果镜像是私有包，需要先登录 GHCR：

```bash
echo <GITHUB_TOKEN> | docker login ghcr.io -u <username> --password-stdin
```

注意：不要把真实 token 写入仓库、文档或提交记录。

## 版本策略

两份工作流共用同一套 version 判定逻辑：

| 事件 | version | Docker is_beta | Docker 别名 |
| --- | --- | --- | --- |
| push 到 `master` | `beta-<sha6>` | true | `beta` |
| push `v*` tag | `github.ref_name` | false | `latest` |
| 手动触发 | 输入框中的 version | false | `latest` |

因此：

- `master` 适合日常 beta
- `v*` 适合正式发版
- **从任意分支手动跑 Docker 构建时，也会更新 `latest`**

从 `dev` 手动触发 Docker 时，建议：

- 只用于临时验证
- version 使用带前缀的临时值，例如 `dev-2026.3.2`
- 或确认你确实要覆盖 `latest`

## 基于 dev 分支能否构建？

### 结论

**可以构建 `dev` 上的代码，但 push `dev` 不会自动触发当前 CI。**

原因：`branches` 只包含 `master`。

| 方式 | 是否可用 | 说明 |
| --- | --- | --- |
| `git push origin dev` | 否 | 不在触发列表 |
| Actions 手动 Run，并选择 branch=`dev` | 是 | 直接基于 `dev` HEAD 构建 |
| 在 `dev` 提交上打 `v*` tag 并 push | 是 | 走正式 tag 路径，Docker 会更新 `latest` |
| 把 `dev` 合入 `master` 再 push | 是 | 走 beta 路径 |
| 本地 `cargo` / `docker build` | 是 | 与 GitHub 分支触发无关 |
| 修改 workflow 把 `dev` 加入 branches | 是 | 需要改配置后才会自动触发 |

### 推荐做法

#### A. 只想验证 `dev` 能否编过

本地最快：

```powershell
cd admin-ui
pnpm install --frozen-lockfile
pnpm build
cd ..
cargo build --release
```

或：

```powershell
docker build -t kiro-rs:dev .
```

#### B. 想要 GitHub 多平台二进制，但不改配置

1. 打开 Actions → **Build Artifacts**
2. **Run workflow**
3. Branch 选择 `dev`
4. Version 填写如 `dev-2026.3.2`
5. 等 run 成功后下载 Artifacts

#### C. 想让 `dev` 持续自动出包

需要改两份 workflow，例如：

```yaml
on:
  push:
    branches:
      - master
      - dev
    tags:
      - 'v*'
  workflow_dispatch:
```

更稳妥的版本区分建议：

| 分支/事件 | 建议 version | 建议 Docker 别名 |
| --- | --- | --- |
| `dev` push | `dev-<sha6>` | `dev` |
| `master` push | `beta-<sha6>` | `beta` |
| `v*` tag | tag 名 | `latest` |

这样 `dev` 不会污染 `beta` / `latest`。

#### D. 正式发布

优先：

1. 把稳定提交合入 `master`
2. 观察 beta 是否成功
3. 打 `v*` tag 并 push
4. 下载二进制 Artifact
5. 手工创建 GitHub Release（当前不会自动创建）

## 日常使用场景

### 1. 日常合入 master

```bash
git checkout master
git pull
git merge dev   # 如需要
git push origin master
```

自动结果：

- 二进制：`kiro-rs-beta-<sha>-...` Artifacts
- 镜像：`ghcr.io/<owner>/kiro-rs:beta`

### 2. 正式发版

```bash
git checkout master
git pull
git tag v2026.3.2
git push origin v2026.3.2
```

自动结果：

- 二进制：`kiro-rs-v2026.3.2-...` Artifacts
- 镜像：`ghcr.io/<owner>/kiro-rs:v2026.3.2` 与 `:latest`

随后：

1. 打开对应 Actions run
2. 下载各平台 Artifact
3. 创建 GitHub Release 并上传这些文件

### 3. 临时补构建 / 指定版本

适合：

- 某平台构建失败后重跑
- 想用自定义 version
- 想基于 `dev` 临时出包

操作：

1. Actions 页面选择对应 workflow
2. **Run workflow**
3. 选择 branch
4. 填写 version

## 权限与前置条件

| 能力 | 需要 |
| --- | --- |
| 二进制构建 | 默认 `GITHUB_TOKEN` 通常足够（`contents: read`） |
| Docker 推送 | `packages: write`；组织仓库可能还需 package 可见性/权限设置 |
| 拉取私有 GHCR 镜像 | 具有 read packages 权限的 token，并 `docker login ghcr.io` |

## 当前工作流未覆盖的事项

1. **不跑测试门禁**  
   没有 `cargo test` / `pnpm test` job。合并前质量检查仍依赖本地或额外 workflow。

2. **不自动创建 GitHub Release**  
   `build.yaml` 只上传 Artifact。

3. **不监听 `dev`**  
   开发分支默认不会自动出包。

4. **手动 Docker 构建会更新 `latest`**  
   从非正式分支手动跑 Docker 时要特别小心。

5. **fork 镜像 owner 可能与 compose 默认值不同**  
   compose 默认 owner 可能是 `wtfdelphia`，而实际推送 owner 取决于当前 GitHub 仓库 owner。

## 快速决策表

| 你的目标 | 建议操作 |
| --- | --- |
| 验证本地代码能编译 | 本地 `cargo build --release` / `docker build` |
| 基于 master 出 beta | push `master` |
| 基于 dev 临时出二进制 | Actions 手动跑 Build Artifacts，branch 选 `dev` |
| 基于 dev 自动持续出包 | 修改 workflow，把 `dev` 加入 branches，并区分 version |
| 正式发版 | 打并推送 `v*` tag |
| 只重出镜像 | Actions 手动跑 Docker workflow |
| 给用户下载安装包 | 下载 Artifact 后手工发 GitHub Release |
| 用 compose 跑镜像 | 设置 `IMAGE_OWNER` / `IMAGE_TAG` 后 `docker compose up` |

## 相关文件

- [`.github/workflows/build.yaml`](../.github/workflows/build.yaml)
- [`.github/workflows/docker-build.yaml`](../.github/workflows/docker-build.yaml)
- [`Dockerfile`](../Dockerfile)
- [`docker-compose.yml`](../docker-compose.yml)
- [`README.md`](../README.md) 中的 Docker / Release 说明


## 3. Build Dev Release（dev 滚动 prerelease）

旁路工作流：[`.github/workflows/build-dev-release.yaml`](../.github/workflows/build-dev-release.yaml)。

与 `build.yaml` 分开：

- `build.yaml`：`master` / `dev` / `v*` 都可出 **Actions Artifact**
- `build-dev-release.yaml`：`dev` 额外自动发 **Releases 首页可见的滚动 prerelease**

| 项 | 行为 |
| --- | --- |
| 触发 | push `dev`；或 Actions 手动 Run |
| 固定滚动 tag | `dev-latest`（默认） |
| 手动 version | workflow_dispatch 可覆盖成一次性 tag |
| 产物 | 7 平台二进制 Artifact + GitHub **Prerelease** Assets |
| 首页可见 | 是，Releases 中固定入口 `dev-latest` |
| 是否覆盖 Latest | 否（`prerelease: true` + `make_latest: false`） |
| tag 策略 | 每次成功构建 `git tag -f dev-latest` 并 force-push，再替换 Assets |
| 权限 | `contents: write` |

### 怎么触发

```bash
git checkout dev
git push origin dev
```

或 Actions → **Build Dev Release** → Run workflow。

成功后固定打开：

- Release: `https://github.com/<owner>/<repo>/releases/tag/dev-latest`
- 资源名示例：`kiro-rs-dev-latest-Windows-x64.exe`

历史一次性 tag（如早期的 `dev-60b58dd`）可以手动删掉，不再自动新增。

### 与 build.yaml 的 dev 行为

| 工作流 | push `dev` 后 |
| --- | --- |
| Build Artifacts | 上传 `kiro-rs-dev-<sha>-...` Artifacts |
| Build Dev Release | 更新滚动 prerelease `dev-latest` |

## 一句话总结

`.github` 当前是发布流水线，不是测试流水线：

- `build.yaml`：多平台二进制打包机
- `docker-build.yaml`：GHCR 多架构镜像打包机

核心用法只有三种：

1. push `master` → beta
2. push `v*` → 正式版
3. Actions 手动填 version → 补跑 / 指定版本 / 临时基于 `dev` 构建
