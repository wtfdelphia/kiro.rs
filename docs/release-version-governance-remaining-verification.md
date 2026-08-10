# release-version-governance 剩余验证执行手册

编写日期：2026-08-10
适用 change：`openspec/changes/release-version-governance`
当前进度：33 项任务中 30 项完成，剩 5.3、7.5、7.6 需外部环境，8.4 需授权

本文档只覆盖本地无法完成的验证。已完成项的证据在
`openspec/changes/release-version-governance/evidence/`，不在此重复。

## 0. 前置：确认目标版本

当前 `Cargo.toml` 与 `Cargo.lock` 均为 `2026.8.10`（按撰写当日 CalVer 取值）。

如果实际发版不在 2026-08-10，必须先改版本再做后续验证，否则 7.5 的绿路径会被门禁
正确地拦下来：

```powershell
# Windows 开发机
# 1. 手工把 Cargo.toml 的 [package].version 改成实际发版日，例如 2026.8.12
cargo update --workspace --offline    # 同步 Cargo.lock
cargo check --release --all-targets   # 必须 0 告警
cargo run --quiet -- --version        # 必须输出 kiro-rs <新版本>
```

严格 CalVer 不补零、无修订后缀：`2026.8.12` 合法，`2026.08.12` 与 `2026.8.12-1` 都会被拒。

> **2026-08-10 格式纠正**：本节「改期就改日期」「同一自然日只能有一个正式版本」已随
> `correct-calver-to-month-sequence` 推翻。版本第三段是**当月发布序号**，不是日历日；同一年月内
> 可发布多个正式版本，序号递增即可，同一天也能发多次。因此上面「把版本改成实际发版日」应读作
> 「把版本改成本月下一个可用序号」。详见 `docs/version-governance-optimization-design.md` 顶部
> 修订记录。

---

## 1. 任务 5.3：Docker 镜像 OCI version label 验证

目标：证明 `docker-build.yaml` 传入的版本会真实落到镜像的
`org.opencontainers.image.version` label 上，而不是停留在 Dockerfile 的默认
`unknown`。

### 1.1 环境要求

- Linux + Docker 20.10 以上（`docker buildx version` 可用更好，但非必需）
- 至少 4 GB 内存、约 6 GB 磁盘：镜像要在容器内编译 Rust release 与 pnpm 前端
- 首次构建约 10-20 分钟，无需任何凭据或密钥

### 1.2 取源码

镜像构建只依赖 `Cargo.toml`、`Cargo.lock`、`src/`、`admin-ui/`，所以工作树必须是待
验证的那个版本。当前改动尚未提交，两种取法：

```bash
# 方式 A：已推送分支后在服务器上克隆
git clone git@github.com:wtfdelphia/kiro.rs.git
cd kiro.rs
git checkout <你的分支>

# 方式 B：直接从开发机同步工作树（不含 .git 与本地产物）
# 在服务器上执行：
rsync -av --exclude '.git' --exclude 'target' --exclude 'node_modules' \
  --exclude 'config.json' --exclude 'credentials*.json' \
  <user>@<devhost>:/path/to/kiro.rs/ ~/kiro-rs-verify/
```

方式 B 的排除项里有 `config.json` 与 `credentials*.json`，不要去掉：它们是被
`.gitignore` 忽略的真实配置，不应离开开发机。

### 1.3 正例：传入版本，label 必须一致

```bash
cd ~/kiro-rs-verify   # 或 clone 出来的目录
VERSION=$(python3 -c "import tomllib;print(tomllib.load(open('Cargo.toml','rb'))['package']['version'])")
echo "cargo version = ${VERSION}"

docker build --build-arg "VERSION=v${VERSION}" -t kiro-rs-verify:label-pass .

docker image inspect kiro-rs-verify:label-pass \
  --format '{{index .Config.Labels "org.opencontainers.image.version"}}'
```

期望输出：`v2026.8.10`（即 `v` + Cargo 版本）。

注意 label 值带 `v` 前缀。这是刻意的：`docker-build.yaml` 传入的是
`steps.version.outputs.version`，正式路径下它等于 git tag 名（含 `v`），镜像 tag 与
label 因此同源。

### 1.4 反例：不传 build-arg，label 必须是 unknown

```bash
docker build -t kiro-rs-verify:label-default .
docker image inspect kiro-rs-verify:label-default \
  --format '{{index .Config.Labels "org.opencontainers.image.version"}}'
```

期望输出：`unknown`。这一步证明 label 真的来自 `ARG`，不是构建器凭空补的常量。

### 1.5 顺带确认镜像内二进制自报版本

这是本 change 的核心动机（历史上镜像内二进制一直自报 `2026.3.1`）：

```bash
docker run --rm kiro-rs-verify:label-pass ./kiro-rs --version
```

期望输出：`kiro-rs 2026.8.10`，与 label 去掉 `v` 后一致。

### 1.6 一次跑完的脚本

```bash
cat > /tmp/verify-oci-label.sh <<'SH'
set -euo pipefail
cd "$1"

VERSION=$(python3 -c "import tomllib;print(tomllib.load(open('Cargo.toml','rb'))['package']['version'])")
TAG="v${VERSION}"

docker build --build-arg "VERSION=${TAG}" -t kiro-rs-verify:label-pass .
docker build -t kiro-rs-verify:label-default .

label() {
  docker image inspect "$1" --format '{{index .Config.Labels "org.opencontainers.image.version"}}'
}

PASS=$(label kiro-rs-verify:label-pass)
DEFAULT=$(label kiro-rs-verify:label-default)
BIN=$(docker run --rm kiro-rs-verify:label-pass ./kiro-rs --version)

echo "--- results ---"
echo "cargo version      : ${VERSION}"
echo "passed label       : ${PASS}   (expect ${TAG})"
echo "default label      : ${DEFAULT}   (expect unknown)"
echo "binary --version   : ${BIN}   (expect kiro-rs ${VERSION})"

test "${PASS}" = "${TAG}"
test "${DEFAULT}" = "unknown"
test "${BIN}" = "kiro-rs ${VERSION}"
echo "OCI version label verification PASSED"
SH

bash /tmp/verify-oci-label.sh ~/kiro-rs-verify
```

### 1.7 需要回填的证据

把三行结果（passed label / default label / binary --version）与
`docker --version` 贴到
`openspec/changes/release-version-governance/evidence/spec-compliance-report.md`
的剩余风险处，并把该风险从「仅静态断言」改为已实测。同时把 tasks.md 的 5.3 勾上。

### 1.8 清理

```bash
docker image rm kiro-rs-verify:label-pass kiro-rs-verify:label-default
docker builder prune -f    # 可选：构建缓存占几 GB
```

### 1.9 用 docker compose 做一次落地验证

仓库根已有 `docker-compose.yml`，它通过 `IMAGE_OWNER` / `IMAGE_TAG` 两个变量选镜像，
不需要为本次验证新写 compose 文件。上面的 1.3 到 1.5 验证的是「镜像元数据正确」，这一
节验证的是「按 README 的方式真正跑起来，版本仍然一致」。

现有定义（保持不变，仅供对照）：

```yaml
services:
  kiro-rs:
    image: ghcr.io/${IMAGE_OWNER:-wtfdelphia}/kiro-rs:${IMAGE_TAG:-latest}
    container_name: kiro-rs
    extra_hosts:
      - "host.docker.internal:host-gateway"
    ports:
      - "8990:8990"
    volumes:
      - ./config/:/app/config/
    restart: unless-stopped
```

#### 验证本地构建的镜像

`image:` 指向 GHCR，而 1.3 构建出的是本地 tag，用一个 override 文件接上去，不改仓库
里的 `docker-compose.yml`：

```bash
cd ~/kiro-rs-verify

cat > docker-compose.override.yml <<'YML'
services:
  kiro-rs:
    image: kiro-rs-verify:label-pass
YML

docker compose config | grep image:      # 确认解析到本地镜像
```

`docker-compose.override.yml` 会被 compose 自动加载，是临时验证的正规做法，验完删掉即可。

#### 准备最小配置

容器的 `CMD` 会去读 `/app/config/config.json` 与 `/app/config/credentials.json`。两者
缺失时代码走的是默认值分支（`Config::load` 返回默认配置，`load_detailed` 返回空凭据
列表），并不会退出，所以只验版本时其实可以不放配置。

但 `./config/` 由 compose 挂载，若宿主机上不存在该目录，Docker 会以 root 建出来，之后
清理麻烦。放一份最小假配置更省事，也更接近真实启动路径：

```bash
mkdir -p config
cat > config/config.json <<'JSON'
{
  "port": 8990
}
JSON
echo '[]' > config/credentials.json
```

这两个文件是**假数据，仅用于本次启动验证**，不要放真实 token 或账号，验完删除。

没有可用凭据时服务仍会监听 8990，只是业务请求会失败。这对本次验证足够：我们只看启动
日志的版本行与顺序。

#### 启动并读第一条日志

这是任务 5.3 与「启动日志可观测」场景的交汇点：正式镜像跑起来后，第一条应用 info 必须
是版本行。

```bash
docker compose up -d
docker compose logs kiro-rs | head -n 5
```

期望第一条应用日志形如：

```
2026-08-10T...  INFO kiro_rs: kiro-rs v2026.8.10
```

要点：它必须出现在任何凭据加载或备份日志**之前**。用上面那份空凭据启动时，紧随其后的
应是「已加载 0 个凭据配置」——版本行在前、凭据行在后，就是本 change 要的顺序。如果凭据
日志先出现，说明 `src/main.rs` 里版本日志的位置不对。

顺序断言在本地已由 `scripts/tests/test_release_governance_files.py::test_startup_reports_version_before_config_error`
覆盖（用损坏配置触发早退，断言首行是版本行）。这里是同一约束在真实容器里的复核。

再交叉核对三处是否同一个版本：

```bash
docker compose exec kiro-rs ./kiro-rs --version
docker image inspect "$(docker compose images -q kiro-rs)" \
  --format '{{index .Config.Labels "org.opencontainers.image.version"}}'
```

`--version` 应为 `kiro-rs 2026.8.10`，label 应为 `v2026.8.10`，日志应为 `kiro-rs v2026.8.10`。
三者去掉 `v` 后必须相等，这就是本 change 要的「同一版本在所有可观测面一致」。

#### 清理

```bash
docker compose down
rm -f docker-compose.override.yml
rm -rf config/            # 删掉上面那份假配置
```

#### 验证正式 GHCR 镜像（任务 7.5 之后）

绿路径跑完、镜像已推到 GHCR 后，同一份 compose 不需要 override 就能验，这也顺带证实
README 里给的启动方式对正式版本有效：

```bash
IMAGE_OWNER=wtfdelphia IMAGE_TAG=v2026.8.10 docker compose up -d
docker compose logs kiro-rs | head -n 3
docker compose exec kiro-rs ./kiro-rs --version
docker compose down
```

顺便确认 `IMAGE_TAG=latest` 解析到的是刚发布的这一版：

```bash
docker pull ghcr.io/wtfdelphia/kiro-rs:latest
docker image inspect ghcr.io/wtfdelphia/kiro-rs:latest \
  --format '{{index .Config.Labels "org.opencontainers.image.version"}}'
```

#### 红路径的 compose 侧确认（任务 7.6 之后）

门禁拦住临时 tag 后，对应镜像根本不该存在。用 compose 拉一次，失败才是正确结果：

```bash
IMAGE_TAG=v2026.8.900 docker compose pull    # 期望：manifest unknown / not found
```

若这条命令成功拉到镜像，说明发布副作用没被前置拦住，属于第 6 节的停止条件。

### 1.10 可选：顺手补 actionlint

本地也缺 actionlint，服务器上顺便跑一次成本很低：

```bash
docker run --rm -v "$(pwd):/repo" -w /repo rhysd/actionlint:latest -color
```

它不是 change 的验收项，但能提前发现 workflow 表达式与上下文引用问题。

---

## 2. 任务 7.5：CI 绿路径

目标：一次真实的正式发布，证明 version gate 通过后产物链完整跑起来。

这一步会产生真实的 GitHub Release 与 GHCR 镜像，不可逆。先确认版本号就是你要发布的
版本。

### 2.1 前置

1. 本 change 的所有改动已合入 `main`（普通 PR 或 merge，不要强推）。
2. `main` 上的 `Cargo.toml` 版本等于目标日期。
3. 记下发版提交 SHA。

```bash
git fetch origin
git log --oneline -1 origin/main
git merge-base --is-ancestor <release-commit> origin/main && echo "reachable from main"
```

### 2.2 创建附注 tag 并推送

必须是附注 tag（`-a`）。轻量 tag 会被门禁拒绝，这是 D1 的刻意约定。

```bash
git tag -a v2026.8.10 -m "Release v2026.8.10" <release-commit>
git push origin v2026.8.10
```

### 2.3 观察两条流水线

推 tag 会同时触发 Build Artifacts 与 Build and Push Docker Images。两条都要检查：

```bash
gh run list --workflow build.yaml --limit 3
gh run list --workflow docker-build.yaml --limit 3
gh run view <run-id>    # 看 job graph
```

期望：

- `version-gate` 与 `warning-gate` 都是 success
- `build`、`release`（build.yaml）与 `build`、`manifest`（docker-build.yaml）都启动并成功
- Release 资产名形如 `kiro-rs-v2026.8.10-Linux-x64`
- GHCR 出现 `v2026.8.10` 与 `latest`

### 2.4 顺带完成 7.5 的镜像交叉验证

绿路径跑完后，正式镜像可以直接 inspect，这比任务 5.3 的本地构建更接近生产：

```bash
docker pull ghcr.io/wtfdelphia/kiro-rs:v2026.8.10
docker image inspect ghcr.io/wtfdelphia/kiro-rs:v2026.8.10 \
  --format '{{index .Config.Labels "org.opencontainers.image.version"}}'
docker run --rm ghcr.io/wtfdelphia/kiro-rs:v2026.8.10 ./kiro-rs --version
```

多架构 manifest 的 label 挂在各架构 config 上，`docker image inspect` 拉取当前架构
镜像后即可看到。若要直接看 manifest：

```bash
docker buildx imagetools inspect ghcr.io/wtfdelphia/kiro-rs:v2026.8.10
```

### 2.5 需要回填的证据

两条 workflow 的 run URL，以及上面 inspect 的输出。

---

## 3. 任务 7.6：CI 红路径

目标：证明门禁真的会拦。只验证绿路径不能说明门禁有效——这是 AGENTS.md 明确要求的。

**必须在真实仓库做，因为要触发 workflow。用一个刻意失配的临时 tag，验证完立刻删除。**

### 3.1 制造 Cargo 失配

挑一个与 `Cargo.toml` 版本不同、且是合法 CalVer 的日期。假设 `Cargo.toml` 是
`2026.8.10`，用 `v2026.8.900`（高位序号，不会与真实发布撞号）：

```bash
git fetch origin
git tag -a v2026.8.900 -m "TEMP gate red-path test" origin/main
git push origin v2026.8.900
```

这个 tag 的一切都合法（附注、月份合法、序号合法、main 可达），唯一的问题是 Cargo 版本对不上。
这样能精准隔离出「Cargo 一致性」这一条判定。

### 3.2 期望结果

```bash
gh run list --workflow build.yaml --limit 3
gh run list --workflow docker-build.yaml --limit 3
gh run view <run-id>
```

两条流水线都应满足：

- `version-gate` 为 failure
- `warning-gate` 为 success（两个门禁正交，编译是干净的）
- `build`、`release`、`manifest` 全部 **skipped**，不是 failure，也不是 success
- 失败 annotation 形如：

  ```
  ::error::Cargo.toml version '2026.8.10' does not match release tag 'v2026.8.900';
  set [package].version to '2026.8.900', update Cargo.lock, commit, and recreate the tag
  ```

- GHCR 上没有出现 `v2026.8.900` 任何 tag，GitHub Releases 里没有对应 Release

顺手确认 version gate 没有等 warning gate（任务 7.7 的运行时侧证据）：两个 gate 的
开始时间应该几乎相同，version gate 应在 warning gate 的 20 分钟编译还在跑时就已经
失败。

```bash
gh run view <run-id> --json jobs \
  --jq '.jobs[] | {name, status, conclusion, startedAt, completedAt}'
```

### 3.3 删除临时 tag

验证完立刻清理，否则 tag 列表会留污点：

```bash
git push origin :refs/tags/v2026.8.900
git tag -d v2026.8.900
git ls-remote --tags origin | grep 2026.8.900    # 应无输出
```

失败的 run 记录会留在 Actions 历史里，这没问题，它正是证据本身。

### 3.4 可选：再补两个反例

如果想让红路径覆盖更全，另外两个成本同样低：

```bash
# 轻量 tag（无 -a）：应报 must be an annotated tag
git tag v2026.8.12 && git push origin v2026.8.12

# 非法月份：应报 invalid month 13
# （注：v2026.2.30 曾作为「非法日历日」反例，但第三段已修正为当月序号，30 现为合法序号）
git tag -a v2026.13.1 -m "TEMP" && git push origin v2026.13.1
```

每个验完都按 3.3 删除。这两条本地单测已覆盖（`test_rejects_lightweight_tag`、
`test_rejects_invalid_calendar_date`），CI 侧属加分项，不是必须。

### 3.5 需要回填的证据

两条 workflow 的失败 run URL、annotation 原文、job graph 里 skipped 的截图或
`gh run view --json` 输出、以及临时 tag 已删除的确认。

---

## 4. 任务 8.4：归档

前置：5.3、7.5、7.6 的证据都已回填到 evidence 目录，且 tasks.md 全部勾选。

```bash
openspec validate --all      # 必须 21 passed
```

然后在 Codex 里执行 `openspec-archive-change`，它会把 delta spec 同步到
`openspec/specs/release-version-governance/` 并把 change 移入
`openspec/changes/archive/`。

归档由你确认后触发，实现侧不会自动 push、建 PR、合并或删远端分支。

---

## 5. 建议顺序

1. 先在服务器做任务 5.3。它无副作用、可反复重来，且能在推 tag 前先证实 Docker
   链路正确。
   先做 1.3-1.5 的镜像元数据验证，再做 1.9 的 compose 落地验证：前者证明 label 与
   `ARG` 真的连着，后者证明按 README 的启动方式跑起来后，日志、`--version` 与 label
   三处版本一致。
2. 再做任务 7.6 红路径。它比绿路径更该先做——如果门禁其实拦不住，绿路径的成功毫无
   意义，而红路径唯一的代价是一个随后删掉的临时 tag。
   拦截后按 1.9 末节用 `docker compose pull` 确认镜像确实不存在。
3. 最后做任务 7.5 绿路径。这一步产生真实 Release 与镜像，应在门禁已被证实有效之后
   进行。镜像推上 GHCR 后，用 1.9 的「验证正式 GHCR 镜像」小节复核 `v<version>` 与
   `latest` 两个 tag。
4. 回填证据、勾选 tasks、执行归档。

## 6. 停止条件

出现以下任一情况，停下来而不是绕过：

- 任务 5.3 的 label 与传入值不一致，或反例没有得到 `unknown`
- compose 启动后第一条应用 info 不是 `kiro-rs v<version>`，或它出现在凭据日志之后
- 日志、`--version`、OCI label 三处版本不一致
- 红路径中任何产物 job 不是 skipped（说明 `needs` 接线有漏）
- 红路径中 GHCR 出现了临时版本的镜像（说明发布副作用未被前置拦住）
- 红路径的 `docker compose pull` 居然拉到了临时版本镜像
- 绿路径中 version gate 失败（先看 annotation：多半是 Cargo 版本、tag 类型或 main
  可达性，按提示修正后重建 tag，不要改门禁）
- 需要强推、reset 或删除 `main`/`master` 才能推进
