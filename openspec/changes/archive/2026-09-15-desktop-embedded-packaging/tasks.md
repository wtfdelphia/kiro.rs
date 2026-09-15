# Tasks: desktop-embedded-packaging

## 1. 图标资源

- [x] 1.1 `desktop/assets/icons/`：PIL 生成 1024→16 方形 PNG 组
      （深色底 + 绿点，与 change 6 托盘色板同源），脚本入证据目录
- [x] 1.2 `icon.ico`（多尺寸）供 `nsis.installerIcon`

## 2. 打包配置

- [x] 2.1 `crates/kiro-desktop/Cargo.toml`：`[package.metadata.packager]`
      （productName、identifier、binaries、icons、deb depends、nsis
      installerIcon；键名经 0.11.8 schema 核对为 camelCase）
- [x] 2.2 本地 `cargo packager --release` 出 `.deb`（依赖 1.1）

## 3. 本地验证

- [x] 3.1 `dpkg-deb -I / -c` 结构校验（control 字段、二进制与图标路径）
- [x] 3.2 解包二进制经 `LD_LIBRARY_PATH` 补 sysroot 库，Xvfb 启动到
      「主窗口已创建」（复用 smoke6 资产）

## 4. CI 流水线

- [x] 4.1 `.github/workflows/desktop-release.yaml`：desktop-gate 腿
      （钉 1.97.1、admin-ui 真实构建、apt 依赖、`-D warnings` check）
- [x] 4.2 矩阵三腿：macos dmg / windows nsis / ubuntu deb，
      `cargo install cargo-packager --locked`，sha256，上传
      `desktop-dev-latest` 滚动预发布
- [x] 4.3 签名环境变量映射 + 未签名标注
- [x] 4.4 YAML 语法校验（python yaml 解析；无 actionlint 环境，
      结构自查清单留证据）

## 5. 远端证据

- [x] 5.1 推送 `dev-desktop` 触发：三平台绿路径 + 未签名降级红路径
      （同一次运行）
- [x] 5.2 `desktop-gate` 负例：临时分支注入告警 + `workflow_dispatch`，
      失败证据留档后删分支

## 6. 收尾

- [x] 6.1 文档：设计文档 §10 回填（格式裁剪、依赖清单、签名行为）；
      `desktop/README.md` 打包命令与 secrets 映射表
- [x] 6.2 门禁：桌面零告警复跑 + 根仓确认；`openspec validate`
- [x] 6.3 spec-compliance-check + verification-before-completion + 单提交
