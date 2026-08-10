# Docker OCI 版本 Label 验证（任务 5.3）

Change：`release-version-governance`
日期：2026-08-10
环境：本机 Docker Engine `29.1.3`，linux/amd64，未使用 buildx（经典 builder）
状态：**PASS**（此前记录的 SKIPPED 已由本轮真实构建取代）

## 执行命令与结果

| 命令 | 结果 |
| --- | --- |
| `docker info --format '{{.ServerVersion}}'` | `29.1.3` |
| `docker build --build-arg VERSION=v2026.8.10 -t kiro-rs-version-label-test:v2026.8.10 .` | 成功；`Step 21/24 : RUN ./kiro-rs --version` 输出 `kiro-rs 2026.8.10` |
| `docker image inspect kiro-rs-version-label-test:v2026.8.10 --format '{{json .Config.Labels}}'` | `{"org.opencontainers.image.version":"v2026.8.10"}` |
| `docker build -t kiro-rs-version-label-test:default --label org.opencontainers.image.source=... --label org.opencontainers.image.description="Kiro API proxy" .`（不传 `VERSION`） | 成功 |
| `docker image inspect kiro-rs-version-label-test:default --format '{{json .Config.Labels}}'` | `{"org.opencontainers.image.description":"Kiro API proxy","org.opencontainers.image.source":"https://github.com/example/kiro-rs","org.opencontainers.image.version":"unknown"}` |
| `docker rmi kiro-rs-version-label-test:v2026.8.10 kiro-rs-version-label-test:default` | 两个临时 tag 已删除，未推送任何镜像仓库 |

## 断言映射

| 规格场景 | 证据 |
| --- | --- |
| 镜像携带一致 OCI 版本 | 传入 `VERSION=v2026.8.10` 后 `org.opencontainers.image.version` 精确等于 `v2026.8.10` |
| `ARG VERSION=unknown` 默认值生效 | 不传 build-arg 时 label 为 `unknown`，不会静默产生空值或构建失败 |
| 保留 source/description labels | 与 workflow 一致地通过 `--label` 传入时，三个 label 共存，`VERSION` 未覆盖其他 label |
| 正式二进制报告匹配版本 | 最终 stage smoke test `./kiro-rs --version` 输出 `kiro-rs 2026.8.10`，等于 Cargo 版本 |

## 说明与剩余风险

- 首次构建在 `RUN apk add --no-cache ca-certificates` 处因 `dl-cdn.alpinelinux.org` 超时失败（网络问题，非 Dockerfile 缺陷）；重跑后通过。`LABEL` 步骤在两次构建中均正常执行。
- 本轮只构建 linux/amd64。多架构（arm64）与 `cache-from/to: type=gha` 属 CI 环境行为，未在本机验证。
- 本轮验证的是 Dockerfile 侧 `ARG`/`LABEL` 契约与 build-arg 传递。`docker-build.yaml` 把门禁确认的版本传入 `build-args` 仍由 `scripts/tests/test_release_governance_files.py` 静态断言覆盖，真实值需任务 7.5 的 CI run 佐证。
- 未登录任何镜像仓库、未推送镜像、未创建 manifest。
