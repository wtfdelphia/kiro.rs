# Design: desktop-embedded-packaging

打包、签名、公证流水线。逐决策给出现状、目标实现与取舍。

## D1 工具：cargo-packager 0.11.8（设计文档 §10.1 定案复核）

现状：§10.1 已选 `cargo-packager`。2026-09-15 复核：0.11.8 仍是最新
（crates.io，2025-11-27 发版），配置面源码核实：

- 配置文件 `packager.toml` / `packager.json` / `Cargo.toml` 的
  `[package.metadata.packager]` 三选一；CLI `--config` 指定时
  `--release` / `--profile` 被忽略，因此配置走 `Cargo.toml` metadata，
  命令行仍用 `--release`
- `out_dir` 是产物目录也是 `binaries` 相对路径解析基准
  （`binaries_dir` 未设时）；`Binary.path` 不带 `.exe` 后缀
- macOS 签名原生读 `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD`
  环境变量（`codesign/macos.rs:175-180`），无证书时不签名不报错；
  公证读 `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`
  （`codesign/macos.rs:426-428`）
- Windows 签名走 `windows.certificate_thumbprint` + signtool，无
  证书不签
- icns 由 PNG 列表自动生成（`util.rs:300`），仓库不需要二进制
  icns；NSIS 安装器图标（`installer_icon`）与主程序图标分离

决策：沿用 0.11.8，配置放 `Cargo.toml` metadata（不新增
`packager.toml`，与 manifest 同源少一个文件）。CI 用
`cargo install cargo-packager --locked --version 0.11.8` 钉版安装。

## D2 格式裁剪：dmg / nsis / deb（修订设计文档 §10.2）

现状：§10 提到 `.app`/`.dmg`、`.deb`/`.AppImage`、`.msi`/`.nsis`。

核实（0.11.8 源码）：

- AppImage：构建期从 GitHub 下载 linuxdeploy 三件套
  （`package/appimage/mod.rs:29-38`），运行期依赖 FUSE；CI 与目标
  机都多一层外部依赖，本期收益低
- msi：依赖 WiX 工具集，配置面重

决策：本期三格式——macOS `dmg`（内部先产 `.app`）、Windows `nsis`、
Linux 只打 `deb`。AppImage / msi 留后续（格式是可加项，配置一行）。
deb 本地可用 `dpkg-deb` 全链路验证，是唯一能在本环境端到端走通的
格式。

## D3 图标资源：PIL 生成一组 PNG 入库

现状：无任何图标资源（`desktop/assets` 不存在）；托盘四态是 change 6
运行时生成，不动。

决策：`desktop/assets/icons/` 入库一组方形 PNG（1024 / 512 / 256 /
128 / 64 / 48 / 32 / 16），PIL 一次性生成（脚本随证据保留，产物进
仓库）。设计沿用 change 6 托盘色板：深色底 + 绿色状态点，与运行时
托盘图标同源。派生格式由工具链消化：

- macOS：cargo-packager 从 PNG 生成 icns（`util.rs:326` 起）
- deb：直接取 PNG（`package/deb/mod.rs:54`）
- Windows：PIL 另存 `icon.ico`（多尺寸），供 `nsis.installer_icon`；
  exe 本体的图标与版本资源（winres）不在本期

## D4 签名门控：secrets 在场即生效，缺席降级

现状：仓库无任何签名证书；设计文档 §10.2 定「流水线就绪，证书到位
即生效」。

决策：不写条件分支包装签名——cargo-packager 本身就是「无证书即跳过
签名」的语义（D1 源码证据）。工作流只做两件事：

1. 把 GitHub secrets 映射为工具读的环境变量 / 配置字段（映射表进
   `desktop/README.md`）：
   - macOS 签名：`APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD`
   - macOS 公证：`APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`
   - Windows：`WINDOWS_CERTIFICATE_THUMBPRINT` →
     `windows.certificate_thumbprint`（经环境变量注入配置）
2. 产物命名与 release 说明标注「未签名」（本期所有运行都是）

红路径证据即本期每次真实运行：无 secrets 配置下三平台产出未签名包，
无失败。绿路径同一次运行。另做一次 `desktop-gate` 拦截负例（临时
分支注入告警，`workflow_dispatch` 触发，失败后删分支留证据）。

## D5 CI 结构：门禁腿 + 三平台矩阵

```text
desktop-release.yaml
├─ desktop-gate（ubuntu-latest，钉 1.97.1）
│   ├─ admin-ui 真实 pnpm build（产物内嵌界面，非占位）
│   ├─ apt 安装 GPUI 系统依赖（清单见 change 2 evidence）
│   └─ RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
│      （desktop workspace 内，--locked）
└─ package（needs gate，矩阵三腿，fail-fast: false）
    ├─ macos-latest  → cargo build --release → cargo packager --release（dmg）
    ├─ windows-latest → 同（nsis）
    └─ ubuntu-22.04  → apt 依赖 → 同（deb）
    每腿：sha256sums 产物 → 上传滚动预发布 desktop-dev-latest
```

触发：`push` 到 `dev-desktop`、`workflow_dispatch`（审查修订：不监听 tag，
版本化发布另立）。加 `concurrency` 取消被超越的重叠运行：发布是删除重建
共享的 `desktop-dev-latest`，重叠运行会在 delete+重建序列上互相打穿。
不复用根 `warning-gate.yaml`：根门禁不含桌面系统依赖且口径是根
workspace（`AGENTS.md` 门禁工具链钉版论证同样适用，桌面腿钉版
1.97.1 与根一致）。产物上传沿用 `build-dev-release.yaml` 的滚动
release 模式（softprops/action-gh-release）。

Linux 构建依赖：以 change 2 spike 取证过的 sysroot 包清单为准
（`openspec/changes/desktop-app-shell/evidence/bridge-plan.md`），
不照抄 Zed 全清单——本机就是靠这份清单编译通过的。

## D6 版本号

本期不引入独立版本轨：`workspace.package.version = "0.1.0"` 直接进
包名（`kiro-desktop_0.1.0_amd64.deb` 形态）。正式发版版本策略随
首个公开版本另立。

## 验证策略

- 本地绿路径：`cargo packager --release` 出 `.deb` → `dpkg-deb -I/-c`
  结构校验 → 解包后二进制经 `LD_LIBRARY_PATH` 补 sysroot 库，Xvfb
  启动到「主窗口已创建」
- CI 绿路径：三平台产物构建成功并上传
- CI 红路径：同一运行即未签名降级证据；`desktop-gate` 负例单独一次
  运行（临时分支 + `workflow_dispatch`，证据截图/日志留档后删分支）
- 门禁：桌面 `RUSTFLAGS="-D warnings" cargo check --release
  --all-targets --locked` 零告警；根仓零改动确认

## 回滚

单提交回滚：删工作流文件、`Cargo.toml` metadata 段与 `desktop/assets`，
无运行时行为变化，回滚零副作用。
