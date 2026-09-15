# spec-compliance-report: desktop-embedded-packaging

日期：2026-09-15　审查范围：提交 `feat(desktop): 新增桌面打包与三平台发布流水线`

## 六维审查

| 维度 | 结论 | 说明 |
| --- | --- | --- |
| Scope | PASS | 只改桌面域：`desktop/crates/kiro-desktop/Cargo.toml`（metadata 段）、`desktop/assets/icons/`、`.github/workflows/desktop-release.yaml`、`desktop/README.md`、`docs/desktop-gpui-embedded-design.md` §10、本 change 工件目录。未触碰根仓 src、根流水线、admin-ui |
| Design | PASS | D1 cargo-packager 0.11.8 + metadata 配置、D2 三格式裁剪（CLI `--formats` 驱动）、D3 PIL 图标 + 派生格式、D4 secrets 映射不包装签名、D5 门禁腿 + 三平台矩阵，均按 design.md 落地；实测偏差已回填（§10.4）：camelCase 键名、`Utility` 类目、显式 binaries、`libfontconfig-dev` 补清单、1024 PNG 按 `@2x` 命名、staging 按格式收窄 |
| Scenarios | PASS | 任务 1.1-6.2 全部勾选（6.3 即本报告与验证报告）；每条验收有对应运行证据（evidence/ 目录 + CI run） |
| Project Rules | PASS | OpenSpec 工件齐（.openspec.yaml skip_specs、proposal/design/tasks/evidence）；提交信息过 caveman-commit；文档过 humanizer-zh；无真实凭据入库（secrets 仅在 workflow env 映射，值为空不落地） |
| Verification | PASS | 只报真实运行：本地打包 9/9、双门禁零告警、openspec validate 37 项、CI 绿/红路径均有 run 证据（见 ci 证据文件） |
| README/AGENTS Sync | PASS | `desktop/README.md` 补打包命令与 secrets 映射表；根 `README.md` 无需同步（打包不改根侧启动/构建/API）；设计文档 §10 回填实测 |

## 总体状态：PASS

## 发现项（均已在本 change 内修复）

1. CI apt 清单缺 `libfontconfig-dev`（本机预装掩盖）：首跑门禁腿失败暴露，已补两处（gate + Linux package 腿），证据 `gate-first-run-fontconfig-gap.txt`
2. macOS icns 生成报 `No matching IconType`：tauri-icns 无 (1024,1024,1) 映射，`icon_1024.png` 改名 `icon_512@2x.png`，证据 `macos-icns-no-matching-icontype.txt`
3. release 资产混入非交付物：staging 的 `*.exe` glob 收到原始二进制与 spike 程序，收窄为 `*-setup.exe` 按格式匹配
4. 打包配置键名：`deny_unknown_fields` + camelCase，kebab-case 会被拒绝（提交前修正）；`category` 需在工具枚举内（`Network` 不合法，改 `Utility`）；metadata 模式自动收集全部 bin target，显式 `binaries` 排除 spike

## 剩余风险

- 三平台产物均未经安装器实测（macOS dmg / Windows nsis 只验证构建成功与资产完整；deb 有本地安装结构校验 + 解包启动）
- 签名路径只有「缺席降级」证据，证书在场路径要等真实证书接入时验证
- Windows 自启动注册表路径无 CI 编译以外验证（同 change 6 遗留）
