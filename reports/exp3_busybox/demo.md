# Exp3 Demo: BusyBox 兼容性测试

> 目标：运行 BusyBox QEMU case，展示 PR #349 恢复的 applet 回归通过。

## 环境准备

以下命令默认在 TGOSKits 容器环境中执行。如果已经进入容器，可跳过本节。

```bash
python3 polyglot-agent-harness/scripts/docker-check.py
cd tgoskits
docker pull ghcr.io/rcore-os/tgoskits-container:latest
docker run -it --rm \
  -v "$PWD":/workspace \
  -w /workspace \
  ghcr.io/rcore-os/tgoskits-container:latest
```

## Demo 1: 运行 BusyBox QEMU case

### 命令

```bash
cd /workspace
cargo xtask starry test qemu --arch riscv64 -c busybox
```

### 预期现象

输出包含 BusyBox 汇总：

```text
PASS: 268  FAIL: 0
```

QEMU harness 最终报告 case passed / ok。

## Demo 2: 确认恢复的 applet 列表

### 命令

```bash
cd /workspace
rg -n "chown|cpio|dos2unix|env|link|mkdir|mv|rmdir|split|tail|tar|xxd" \
  test-suit/starryos/normal/qemu-smp1/busybox
```

### 预期现象

输出包含以下 applet 名称：

```text
chown
cpio
dos2unix
env
link
mkdir
mv
rmdir
split
tail
tar
xxd
```

## Demo 3: 确认 success matcher 要求 `FAIL: 0`

### 命令

```bash
cd /workspace
rg -n "FAIL: 0" test-suit/starryos/normal/qemu-smp1/busybox
```

### 预期现象

输出中能看到 QEMU 配置或测试配置要求：

```text
FAIL: 0
```
