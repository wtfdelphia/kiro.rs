# kiro-rs 桌面端（desktop/）

独立 workspace，不加入根 workspace（原因见
`docs/desktop-gpui-embedded-design.md` §3.1：根门禁环境没有 GPUI 的
Linux 系统依赖，合并会打穿服务端发布路径）。

当前已完成到 change 6（`desktop-tray-resident`）：change 2 骨架、
change 3 SQLite 存储、change 4 凭据视图（列表/筛选/启停/删除/测试/
余额 + 添加/批量导入/ KAM 导入对话框 + Builder ID / IAM SSO 在线
登录）、change 5 设置面板与内嵌服务器（启停/重启/改地址即时重启 +
两阶段退出 + 标题栏实时状态徽标）、change 6 系统托盘与关窗常驻
（`tray-icon` 0.25 ksni 后端，无 SNI 宿主自动降级；关窗最小化常驻 +
托盘唤回；开机自启三平台入口；设置「行为」分区），另有审核修复
（`desktop-review-followups`）。
凭据与配置默认落 `<数据目录>/kiro.db`（WAL），凭据 secret 字段走系统
钥匙串（`keyring`），无 Secret Service 的环境回退加密文件（`secrets.enc`）。
显式 `--config` / `--credentials` 时仍走 JSON 文件（等价 CLI，调试用）。

## 构建前提

- Rust 1.97.1+（与根仓一致）
- `admin-ui/dist` 存在（path 依赖会触发根仓 `rust-embed` 编译；本机已构建
  过或建占位目录均可）
- Linux 系统库（见下方清单）

### Linux 系统依赖

GPUI（`gpui-pre`）的 Linux 构建需要这些开发包：

```text
libxkbcommon-x11-dev  libxkbcommon-x11-0（运行时）
libasound2-dev        libasound2-dev 依赖的运行时系统已有
libzstd-dev           运行时系统已有
libx11-xcb-dev        运行时系统已有
libxcb-xkb-dev        libxcb-xkb1（运行时）
libvulkan-dev         运行时系统已有（Vulkan ICD 需可用）
```

有 root 权限时：

```bash
sudo apt install libxkbcommon-x11-dev libasound2-dev libzstd-dev \
    libx11-xcb-dev libxcb-xkb-dev libvulkan-dev
```

### 无 root 环境：本地 sysroot

没有 sudo 时，用 `apt-get download` 下载 deb 包解出本地目录，改写
`.pc` 前缀后并入 `PKG_CONFIG_PATH`（不用 `PKG_CONFIG_SYSROOT_DIR`，
避免把系统库解析也指进 sysroot）：

```bash
SYS=$HOME/kiro-sysroot            # 任意路径
mkdir -p $SYS
cd $SYS
apt-get download libxkbcommon-x11-dev libxkbcommon-x11-0 \
    libasound2-dev libzstd-dev libx11-xcb-dev libxcb-xkb-dev libxcb-xkb1 \
    libvulkan-dev
for d in *.deb; do dpkg-deb -x "$d" $SYS; done
# 把 .pc 的 prefix 改写为绝对路径（本机验证过的步骤）
for f in $SYS/usr/lib/x86_64-linux-gnu/pkgconfig/*.pc; do
    sed -i "s|^prefix=/usr\$|prefix=$SYS/usr|" "$f"
done
export PKG_CONFIG_PATH=$SYS/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH
# 链接期：xkbcommon crate 用裸 #[link] 标志，需要搜索路径
export LIBRARY_PATH=$SYS/usr/lib/x86_64-linux-gnu:$LIBRARY_PATH
# 运行期：系统缺少的 .so.0（xkbcommon-x11、xcb-xkb）从 sysroot 解析
export LD_LIBRARY_PATH=$SYS/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH
```

## 常用命令

```bash
cd desktop

# 编译门禁（与根侧 warning-gate 等价的逐 flag 口径）
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked

# 测试
cargo test --release

# 无显示环境下启动验证（Xvfb + Vulkan 软渲染 ICD）
xvfb-run -a target/release/kiro-desktop \
    --config <config.json> --credentials <credentials.json>
```

`--config` / `--credentials` 显式指定时走 JSON 文件存储（等价 CLI 行为，
便于调试）；未指定时走 SQLite 存储。

### 首启 JSON 导入

SQLite 库为空（无凭据且无配置）时，首启按顺序探测两个目录下的
`config.json` / `credentials.json`：

1. 数据目录（`KIRO_RS_DATA_DIR` 或默认 `~/.local/share/kiro-rs/`）
2. 启动时的工作目录（cwd）

命中即导入，成功后源文件改名为 `config.json.bak` /
`credentials.json.bak`（同名已存在时覆盖）。cwd 命中意味着启动目录下
有这两个文件会被搬走，日志会以 warn 记录来源路径；不想让桌面端碰
工作目录文件时，从别的目录启动，或先把文件挪进数据目录。

数据目录：Linux `$XDG_DATA_HOME/kiro-rs/`（缺省 `~/.local/share/kiro-rs/`），
可用 `KIRO_RS_DATA_DIR` 覆盖。目录内含：

| 文件 | 内容 |
| --- | --- |
| `kiro.db` / `-wal` / `-shm` | SQLite 主存储（凭据、配置、模型目录） |
| `secrets.enc` / `secrets.key` | 钥匙串不可用时的加密文件回退（0600） |
| `kiro-desktop.log` / `kiro-desktop.lock` | 日志与单实例锁（change 2） |

## 打包（change 7）

打包走 `cargo-packager`（0.11.8），配置在 `crates/kiro-desktop/Cargo.toml`
的 `[package.metadata.packager]`；格式经 `--formats` 按平台驱动，
只打 `dmg`（macOS）/ `nsis`（Windows）/ `deb`（Linux），不做 AppImage
（需 FUSE + 构建期外部下载）。产物落在 `target/release/`。

```bash
cd desktop

# 一次性装工具（钉版）
cargo install cargo-packager --version 0.11.8 --locked

# 先构建再打包（Linux 示例；产物 *.deb）
cargo build --release --locked
cargo packager --release --formats deb

# 结构校验 + 解包启动（脚本在 openspec/changes/desktop-embedded-packaging/evidence/）
dpkg-deb -I target/release/kiro-desktop_0.1.0_amd64.deb
```

deb 运行时依赖（ldd 实测写入 Depends）：`libxau6`、`libxdmcp6`、
`libbsd0`、`libmd0`、`libxcb1`、`libxcb-xkb1`、`libxkbcommon0`、
`libxkbcommon-x11-0`。

### 签名 secrets 映射（`.github/workflows/desktop-release.yaml`）

签名走 cargo-packager 原生语义：secrets 在场即签名 + 公证，缺席产出
未签名包不报错。本期所有产物均为未签名构建。

| GitHub secret | 工具读取的环境变量 | 作用 |
| --- | --- | --- |
| `APPLE_CERTIFICATE` | `APPLE_CERTIFICATE` | macOS 签名证书（base64 p12） |
| `APPLE_CERTIFICATE_PASSWORD` | `APPLE_CERTIFICATE_PASSWORD` | 证书口令 |
| `APPLE_ID` | `APPLE_ID` | 公证账号 |
| `APPLE_PASSWORD` | `APPLE_PASSWORD` | 应用专用密码 |
| `APPLE_TEAM_ID` | `APPLE_TEAM_ID` | 团队 ID |

Windows 签名本期不接：工具只读 `certificate_thumbprint` 配置字段，
无环境变量通道（见设计文档 §10.2）。三平台产物附 `SHA256SUMS-*`，
上传滚动预发布 `desktop-dev-latest`。

## 告警纪律

与根仓一致：零新增编译告警。判定命令即上面的 `cargo check --release
--all-targets --locked` 加 `RUSTFLAGS="-D warnings"`。desktop 自带这套判定，
不借用根门禁；根侧流水线零改动。

## 后续

- 签名证书到位后接入（macOS 签名 + 公证；Windows 经
  `certificate_thumbprint` 配置）
- AppImage / msi 格式（可加项，配置一行）
- 自动更新（cargo-packager 更新器协议）
- 正式发版版本策略（随首个公开版本另立）
