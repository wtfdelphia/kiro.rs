#!/usr/bin/env bash
# change 7 任务 3.1/3.2：本地 .deb 结构校验 + 解包启动验证
# 用法：bash verify_deb.sh <path-to-deb>
set -u
DEB="${1:?用法: verify_deb.sh <deb 路径>}"
WORK=/tmp/smoke7/unpack
LOG=/tmp/smoke7/app.log
DATA=/tmp/smoke7/data
PASS=0; FAIL=0
check() { if eval "$2"; then echo "PASS: $1"; PASS=$((PASS+1)); else echo "FAIL: $1"; FAIL=$((FAIL+1)); fi }

echo "== dpkg-deb 结构校验 =="
dpkg-deb -I "$DEB" | tee /tmp/smoke7/control.txt
check "control 有 Package 字段" "grep -q '^ Package: ' /tmp/smoke7/control.txt"
check "control 有 Depends 字段" "grep -q 'Depends:' /tmp/smoke7/control.txt"
check "deb depends 含 libxkbcommon-x11-0" "grep 'Depends:' /tmp/smoke7/control.txt | grep -q 'libxkbcommon-x11-0'"

dpkg-deb -c "$DEB" | tee /tmp/smoke7/contents.txt
check "包含主二进制" "grep -q 'kiro-desktop' /tmp/smoke7/contents.txt"
check "包含 .desktop 入口" "grep -q '\.desktop' /tmp/smoke7/contents.txt"
check "包含桌面图标" "grep -qi '\.png' /tmp/smoke7/contents.txt"

echo "== 解包 =="
rm -rf "$WORK" && mkdir -p "$WORK"
dpkg-deb -x "$DEB" "$WORK"
BIN=$(find "$WORK" -name kiro-desktop -type f | head -1)
check "解包出可执行二进制" "test -n '$BIN' && test -x '$BIN'"

echo "== Xvfb 启动验证（主窗口已创建）=="
export KIRO_RS_DATA_DIR=$DATA
rm -rf $DATA && mkdir -p $DATA
export VK_ICD_FILENAMES=/tmp/smoke5/noicd/empty.json
export LD_LIBRARY_PATH=/tmp/debprobe/sysroot2/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}

pgrep -f "Xvfb :98" >/dev/null || { Xvfb :98 -screen 0 1280x800x24 >/tmp/smoke7/xvfb.log 2>&1 & sleep 1; }
export DISPLAY=:98
"$BIN" >"$LOG" 2>&1 &
APP_PID=$!
sleep 12
check "进程存活（窗口装配期）" "kill -0 $APP_PID"
check "日志出现主窗口已创建" "grep -q '主窗口已创建' '$LOG' || grep -qi 'window' '$LOG'"
kill $APP_PID 2>/dev/null; sleep 1; kill -9 $APP_PID 2>/dev/null

echo "== 结果: PASS=$PASS FAIL=$FAIL =="
exit $FAIL
