# Proposal: desktop-embedded-packaging

桌面版（路线 B，`docs/desktop-gpui-embedded-design.md` §12 里程碑 7）：
打包、签名、公证流水线。完成后桌面应用从源码构建走向可分发：三平台
安装包产出，证书到位即可签名，流水线零改动生效。

## Why

- change 2 至 6 已具备完整功能形态（凭据管理、内嵌服务器、托盘
  常驻），但分发路径只有 `cargo build`。桌面应用的交付形态是安装
  包：图标、菜单入口、卸载、版本标识都要靠打包层提供。
- 设计文档 §10 早已定案 `cargo-packager` 与三平台矩阵，本期落地；
  签名依赖证书密钥，本期目标是「流水线就绪，证书到位即生效」，不
  采购证书（设计文档 §10.2）。
- 里程碑表第 7 行的「自动启动服务器收尾」已由 change 6 的
  `auto_start_server` 偏好覆盖，本 change 不重复建设。

## What Changes

- `desktop/crates/kiro-desktop/Cargo.toml`：`[package.metadata.packager]`
  打包配置（产品名、identifier、格式、图标、deb 运行时依赖）。
- `desktop/assets/icons/`（新增）：应用图标 1024→16 一组 PNG，
  PIL 生成入库；macOS icns 由 cargo-packager 从 PNG 自动生成，
  不引二进制格式。托盘四态图标维持 change 6 的运行时生成，不变。
- `.github/workflows/desktop-release.yaml`（新增）：
  1. `desktop-gate`：钉 1.97.1，`RUSTFLAGS="-D warnings"` +
     `cargo check --release --all-targets --locked`（desktop workspace
     口径），`admin-ui/dist` 占位，Linux 系统依赖 apt 步骤；红则
     不出产物
  2. 三平台矩阵构建 + 打包：`macos-latest`（dmg/app）、
     `windows-latest`（nsis）、`ubuntu-22.04`（deb）
  3. 签名门控：secrets 存在走签名/公证（cargo-packager 原生读
     `APPLE_CERTIFICATE` 等环境变量与配置项），缺失产出未签名包，
     release 说明明示
  4. 产物上传滚动预发布 `desktop-dev-latest`，附 sha256
- 文档：设计文档 §10 回填实测结论（格式裁剪、依赖清单、签名行为）；
  `desktop/README.md` 补打包命令与产物说明。

## 非目标

- 自动更新（cargo-packager updater）、AppImage / msi / wix / pacman
  格式（AppImage 依赖运行时 FUSE 与构建期外部下载，本期只打 deb）
- 证书采购与真实签名运行（无证书环境只验证降级路径）
- macOS x64 / Linux arm64 额外架构（矩阵三平台各一架构起步）
- 托盘图标位图资源（change 6 已用运行时生成定案）

## 影响面

- 新增：一个工作流文件、打包配置段、图标资源目录
- 不动：根仓三条流水线、`warning-gate.yaml`、根仓代码与桌面功能代码
- 风险类型：发布 / CI（`AGENTS.md` 高风险矩阵），需绿路径与红路径
  双 run 证据

## 成功标准

1. 本地：cargo-packager 打出 `.deb`，`dpkg-deb` 结构校验通过，解包
   后二进制可在 Xvfb 启动（经 LD_LIBRARY_PATH 补齐系统库）
2. CI：三平台产物构建成功并上传（绿路径）；同一次运行即未签名降级
   路径证据（红路径）；`desktop-gate` 拦截告警的负例证据
3. 门禁：根仓与桌面零新增告警；`openspec validate` 通过
