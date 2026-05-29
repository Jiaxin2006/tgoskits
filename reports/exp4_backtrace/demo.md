# Exp4 Demo: Backtrace raw block + host symbolize

> 目标：展示 ArceOS raw backtrace、host-side symbolize，以及 StarryOS memtrack-backtrace E2E 的预期输出。
>
> 前提：raw unwind 依赖帧指针。需确保已包含 frame-pointer config-key 修复
> （`fix/memtrack-raw-backtrace` 的 `a77b21966`）；否则受 #839 回归影响，所有 raw
> backtrace 会退化成只展开一帧（`BT 0 ip=0x4 fp=0xffffffffffffffff`）。
> 快速自检：`cargo build --verbose ... | grep force-frame-pointers` 应有命中。

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

Starry QEMU 测试需要 `debugfs`。进入容器或宿主环境后可检查：

```bash
which debugfs
```

预期现象：

```text
/usr/sbin/debugfs
```

如果 macOS 宿主机运行且 `which debugfs` 为空，可先执行：

```bash
export PATH="/opt/homebrew/opt/e2fsprogs/sbin:$PATH"
```

## Demo 1: ArceOS 自动 host symbolize

### 命令

```bash
cd /workspace
cargo xtask arceos test qemu --arch x86_64 --test-group rust --test-case backtrace-raw-normal
```

### 预期现象

QEMU guest 输出 raw block：

```text
BACKTRACE_BEGIN kind=raw arch=x86_64
BT 0 ip=0x... fp=0x...
BT 1 ip=0x... fp=0x...
BACKTRACE_END
```

host 侧随后输出：

```text
=== host backtrace symbolize ===
```

并显示 demangled 函数名或源码位置。

## Demo 2: StarryOS memtrack-backtrace E2E

### 命令

```bash
cd /workspace
cargo xtask starry test qemu --arch x86_64 --test-group normal --test-case memtrack-backtrace
```

### 预期现象

guest 侧输出包含：

```text
Memory allocation sample recorded
Hard memory allocation sample recorded
BACKTRACE_BEGIN kind=alloc arch=x86_64 alloc=true dwarf=false
BT 0 ip=0x... fp=0x...
BACKTRACE_END
STARRY_MEMTRACK_BACKTRACE_OK
```

host 侧输出包含：

```text
=== host backtrace symbolize ===
BT 0 ip=0x... fp=0x0 starry_memtrack_symbolize_probe
ok: memtrack-backtrace
```

其中 `symbolize` probe 是确定性单帧（`fp=0x0` 预期），作为 CI 硬断言。

而 `sample_hard` 制造的真实 alloc 调用链，在帧指针修复后会被展开为多帧函数名
（观察项，非硬断言）：

```text
=== host backtrace symbolize ===
BACKTRACE_BLOCK 0 kind=alloc arch=x86_64
BT 0  ip=0x... starry_memtrack_sample_hard_leaf
BT 1  ip=0x... starry_memtrack_sample_hard_mid
BT 2  ip=0x... <probe handler>
...   ip=0x... ax_fs_ng / ax_task 调度链
BT N  ip=0x0 fp=0x0
```

> 对比：若缺少帧指针修复（#839 回归），`sample_hard` 的 `kind=alloc` 块会是空块
> 或只有 `BT 0 ip=0x0 fp=0x0`，host 侧也就无法解析出 `leaf`/`mid` 函数名。
> 这正是当初误判"allocator FP unwind 固有不稳定"的来源。
> 注意 `BACKTRACE=y` 只给函数名、无源码行号；要行号需 `DWARF=y`。

## Demo 3: 手动保留 raw log 后 symbolize

### 命令

```bash
cd /workspace
cargo xtask arceos test qemu \
  --arch x86_64 \
  --test-group rust \
  --test-case backtrace-raw-normal \
  --keep-qemu-log
```

假设 log 路径为：

```text
tmp/axbuild/qemu-logs/backtrace-raw-normal-x86_64-unknown-none.log
```

继续执行：

```bash
cargo xtask backtrace symbolize \
  --elf target/x86_64-unknown-none/release/arceos-backtrace-raw-normal \
  --log tmp/axbuild/qemu-logs/backtrace-raw-normal-x86_64-unknown-none.log \
  --kind raw
```

### 预期现象

手动 symbolize 输出包含：

```text
BACKTRACE_BLOCK 0 kind=raw arch=x86_64
BT 0 ip=0x...
```

并显示函数名或源码位置。

## Demo 4: 关闭自动 symbolize

### 命令

```bash
cd /workspace
cargo xtask arceos test qemu \
  --arch x86_64 \
  --test-group rust \
  --test-case backtrace-raw-normal \
  --no-symbolize
```

### 预期现象

QEMU guest 输出 raw block：

```text
BACKTRACE_BEGIN kind=raw arch=x86_64
BT 0 ip=0x... fp=0x...
BACKTRACE_END
```

终端不出现：

```text
=== host backtrace symbolize ===
```
