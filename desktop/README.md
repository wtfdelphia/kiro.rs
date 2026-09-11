# kiro-rs 桌面端（desktop/）

独立 workspace，不加入根 workspace（原因见
`docs/desktop-gpui-embedded-design.md` §3.1：根门禁环境没有 GPUI 的
Linux 系统依赖，合并会打穿服务端发布路径）。

当前是 change 2（`desktop-app-shell`）的应用骨架：双运行时、事件桥、
数据目录、单实例锁、窗口 + sidebar + 四占位视图。无业务功能。

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
便于调试）；未指定时读数据目录下的 `config.json` / `credentials.json`。
数据目录：Linux `$XDG_DATA_HOME/kiro-rs/`（缺省 `~/.local/share/kiro-rs/`），
可用 `KIRO_RS_DATA_DIR` 覆盖。

## 告警纪律

与根仓一致：零新增编译告警。判定命令即上面的 `cargo check --release
--all-targets --locked` 加 `RUSTFLAGS="-D warnings"`。desktop 自带这套判定，
不借用根门禁；根侧流水线零改动。

## 后续

- change 3：SQLite 存储替换 JSON store
- change 4/5：凭据视图、设置与服务器视图
- change 6：托盘与常驻
- change 7：打包与三平台 CI
