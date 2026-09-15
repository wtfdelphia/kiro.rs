# Bridge Plan: desktop-embedded-packaging

门禁：`openspec-superpowers-bridge`。

- change：`desktop-embedded-packaging`
- 分支：`dev-desktop`（HEAD `24e9ef9`，工作区干净；本地领先远端
  2 个提交：change 5/6 未推送）
- 状态：`openspec status` 非 blocked，validate 通过

## 一、范围 / 非目标

范围：`cargo-packager` 0.11.8 打包配置（dmg / nsis / deb 三格式）、
应用图标资源、三平台发布流水线（含告警门禁腿与签名环境变量映射）、
本地 `.deb` 端到端验证。

非目标：AppImage / msi / wix / pacman；自动更新；证书采购与真实
签名运行；exe 本体图标与版本资源（winres）；macOS x64 与 Linux
arm64 扩展架构；独立版本轨。

## 二、关键设计决策（design.md 摘要）

- D1 沿用 0.11.8，配置放 `Cargo.toml` metadata；签名缺席时工具自身
  跳过，流水线不写条件分支
- D2 格式裁剪 dmg / nsis / deb；AppImage 外部下载 + FUSE 依赖，
  本期不做
- D3 图标：PIL 生成 PNG 组入库，icns / ico 由工具链派生
- D4 签名门控：secrets 映射为环境变量，缺席产出未签名包并在
  release 标注
- D5 CI：`desktop-gate`（钉 1.97.1）+ 三平台矩阵；滚动预发布
  `desktop-dev-latest`

## 三、风险与对策

| 风险 | 对策 |
| --- | --- |
| GitHub Actions 无法本地复现，证据依赖推送后运行 | 本地先走通 `.deb` 全链路；CI 失败按日志迭代，绿路径与红路径分开留档 |
| Linux 腿系统依赖清单不全导致编译失败 | 以 change 2 spike 实证过的包清单为准（`desktop-app-shell/evidence/bridge-plan.md`），不猜 |
| 推送触发影响远端仓库 | 触发面限 `dev-desktop` 分支与 `workflow_dispatch`；负例用临时分支，跑完删除 |
| `cargo install cargo-packager` 在 CI 耗时 | 钉版 0.11.8 + `--locked` + rust-cache；构建主体本就 10 分钟级 |
| 未签名产物被误当正式分发 | 产物名与 release 说明明示未签名 |

## 四、CodeGraph 证据与补盲

- `codegraph status`：索引覆盖根仓；本 change 影响面全在 `desktop/`
  与 `.github/`，补盲以源码核实为主
- 源码核实（cargo-packager 0.11.8 crates.io 源码包，`/tmp/pkg-src/`）：
  签名环境变量读取（`codesign/macos.rs:175-180,426-428`）、icns
  PNG 派生（`util.rs:300`）、AppImage 外部下载（`package/appimage/
  mod.rs:29-38`）、deb 图标（`package/deb/mod.rs:54`）、CLI
  `--release` 与 `--config` 互斥（`cli/mod.rs:73-81`）
- `rg` 补盲：现有流水线风格（`build.yaml` 矩阵与产物上传、
  `build-dev-release.yaml` 滚动 release、`warning-gate.yaml` 钉版与
  占位目录）；Linux 系统依赖清单（change 2 bridge-plan）
- 环境事实：本机有 `dpkg-deb`、PIL、`/tmp/debprobe/sysroot2`
  （change 2 编译取证依赖）、Xvfb 冒烟资产（`/tmp/smoke6`）；
  无 `actionlint` / 免密 sudo，YAML 校验用 python 解析 + 结构自查

## 五、任务到执行步骤

| 任务 | 执行步骤 | 验证 |
| --- | --- | --- |
| 1.1/1.2 图标 | PIL 脚本生成 PNG 组 + ico 入 `desktop/assets/icons/` | 文件尺寸断言在脚本内 |
| 2.1 配置 | `Cargo.toml` metadata：formats=[dmg,nsis,deb] 按平台、binaries、icons、deb depends | `cargo packager --release` 可解析 |
| 2.2 打包 | 本地 deb 腿 | 产物存在 |
| 3.1 结构 | `dpkg-deb -I` / `-c` | control 字段与路径正确 |
| 3.2 可启动 | 解包 + `LD_LIBRARY_PATH` + Xvfb | 日志到「主窗口已创建」 |
| 4.1–4.4 流水线 | 写 `desktop-release.yaml`；python yaml 解析；结构自查清单 | 语法与结构证据 |
| 5.1 远端绿/红 | 推送触发，下载产物与日志留档 | 三平台产物 + 未签名标注 |
| 5.2 门禁负例 | 临时分支注入告警 + `workflow_dispatch` | gate 失败日志留档后删分支 |
| 6.1–6.3 收尾 | 文档回填、门禁复跑、合规审查、单提交 | 门禁全绿 |

## 六、必跑验证

1. 本地：`cargo packager --release`（deb）→ `dpkg-deb -I/-c` →
   Xvfb 启动
2. 桌面门禁：`RUSTFLAGS="-D warnings" cargo check --release
   --all-targets --locked` 零告警（打包配置改动后复跑）
3. 根仓门禁确认（预期零根仓代码改动）
4. `openspec validate desktop-embedded-packaging`
5. CI 远端运行证据（绿 + 红）与门禁负例证据
6. 提交前 `git status --short`

## 七、README / spec 同步判断

- `desktop/README.md`：补打包命令、产物形态、secrets 映射表（影响
  构建与分发入口，必须同步）
- 设计文档 §10：回填格式裁剪与签名行为的实测结论
- 根 `README.md` / `AGENTS.md` / 主 specs：不动（桌面小节已指向子
  文档；`skip_specs: true`）

## 八、停止条件

- 本地 `.deb` 全链路走不通（工具或配置缺陷）且无法归因
- CI 三平台连续失败且失败原因指向方案本身（如 ubuntu 依赖清单
  与 GitHub runner 不兼容）
- 门禁出现新增告警且无法归因
- 远端运行证据拿不到（网络 / 权限），则如实记录 SKIPPED 与剩余风险
