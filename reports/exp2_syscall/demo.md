# Exp2 Demo: StarryOS Syscall 与边界语义

> 目标：展示三个 StarryOS PR 对应测试的可复现入口和预期现象。

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

## Demo 1: PR #265 `unlinkat` invalid flags

### 命令

```bash
cd /workspace
cargo xtask starry test qemu --arch riscv64 -c bug-unlinkat-einval
```

可选多架构：

```bash
cargo xtask starry test qemu --arch aarch64 -c bug-unlinkat-einval
cargo xtask starry test qemu --arch x86_64 -c bug-unlinkat-einval
cargo xtask starry test qemu --arch loongarch64 -c bug-unlinkat-einval
```

### 预期现象

输出包含：

```text
TEST PASSED
```

或最终 case 状态为 passed / ok。

## Demo 2: PR #348 tmpfs hardlink cache

### 命令

```bash
cd /workspace
cargo xtask clippy --package axfs-ng-vfs
cargo xtask starry test qemu --arch riscv64 -c bug-tmpfs-hardlink-cache
```

### 预期现象

clippy 无 error。

QEMU 测试输出包含：

```text
TEST PASSED
```

或最终 case 状态为 passed / ok。

## Demo 3: PR #350 `getrandom` / `prlimit64` boundary tests

### 命令

```bash
cd /workspace
cargo xtask starry test qemu --arch riscv64 -c test-getrandom
cargo xtask starry test qemu --arch riscv64 -c test-prlimit64
```

### 预期现象

两个 QEMU case 均输出：

```text
TEST PASSED
```

或最终 case 状态为 passed / ok。
