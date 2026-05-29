# Task1 Demo: ArceOS Tutorial

> 目标：运行五个 `exercise-*` 的测试脚本，展示 Task1 五个实验均已完成。

## 环境准备

以下命令默认在 TGOSKits/ArceOS 容器环境中执行。如果已经进入容器，可跳过本节。

```bash
python3 polyglot-agent-harness/scripts/docker-check.py
docker pull ghcr.io/rcore-os/tgoskits-container:latest
docker run -it --rm \
  -v "$PWD":/workspace \
  -w /workspace \
  ghcr.io/rcore-os/tgoskits-container:latest
```

进入容器后确认五个 exercise 目录存在：

```bash
cd /workspace/tg-arceos-tutorial
find . -maxdepth 1 -type d -name 'exercise-*' | sort
```

预期现象：

```text
./exercise-altalloc
./exercise-hashmap
./exercise-printcolor
./exercise-ramfs-rename
./exercise-sysmap
```

## Demo 1: 批量运行五个实验测试

### 命令

```bash
cd /workspace/tg-arceos-tutorial
./scripts/batch_exercise_exec.sh -c "bash scripts/test.sh"
```

### 预期现象

五个实验都会进入各自的测试脚本，输出类似：

```text
=== ArceOS Printcolor Exercise Test ===
...
✓ All exercise tests passed!

=== ArceOS HashMap Exercise Test ===
...
✓ All exercise tests passed!

=== ArceOS Altalloc Exercise Test ===
...
✓ All exercise tests passed!

=== ArceOS Ramfs-Rename Exercise Test ===
...
✓ All exercise tests passed!

=== ArceOS Sysmap Exercise Test ===
...
✓ All exercise tests passed!
```

每个测试脚本最后都有汇总：

```text
=================SUMMARY=================
  Passed:  <n> / 4
  Failed:  0 / 4
  Skipped: <m> / 4
=========================================
✓ All exercise tests passed!
```

如果某些架构的 QEMU 不存在，脚本会显示 skipped；只要至少一个架构实际运行、`Failed: 0 / 4`，并出现 `✓ All exercise tests passed!`，该 exercise 的测试通过。

## 常见复现问题: `__PERCPU_SELF_PTR` duplicate symbol

如果 `exercise-printcolor` 或 `exercise-altalloc` 在 x86_64 下没有跑出预期文本，并且日志最后出现：

```text
rust-lld: error: duplicate symbol: __PERCPU_SELF_PTR
```

这通常不是实验源码输出写错，而是依赖锁漂移导致同一目标里同时链接了两个 `percpu` 版本。可先检查：

```bash
cd /workspace/tg-arceos-tutorial/exercise-printcolor
cargo tree -i percpu --target x86_64-unknown-none
```

如果提示存在 `percpu@0.2.3-preview.1` 和 `percpu@0.4.0` 两个版本，需要恢复 lockfile 一致性。当前可通过确认 `axcpu` 版本来判断：

```bash
cd /workspace/tg-arceos-tutorial
grep -A2 'name = "axcpu"' exercise-printcolor/Cargo.lock
grep -A2 'name = "axcpu"' exercise-altalloc/Cargo.lock
```

预期应为：

```text
name = "axcpu"
version = "0.3.0-preview.6"
```

修复后建议在容器里完整跑五个实验，避免宿主机工具链或 QEMU 差异影响结果：

```bash
cd /workspace/tg-arceos-tutorial
for d in exercise-printcolor exercise-hashmap exercise-altalloc exercise-ramfs-rename exercise-sysmap; do
  echo "===== $d ====="
  (cd "$d" && bash scripts/test.sh) || exit 1
done
```

## Demo 2: 分别运行五个实验测试

### 命令

```bash
cd /workspace/tg-arceos-tutorial/exercise-printcolor
bash scripts/test.sh

cd /workspace/tg-arceos-tutorial/exercise-hashmap
bash scripts/test.sh

cd /workspace/tg-arceos-tutorial/exercise-altalloc
bash scripts/test.sh

cd /workspace/tg-arceos-tutorial/exercise-ramfs-rename
bash scripts/test.sh

cd /workspace/tg-arceos-tutorial/exercise-sysmap
bash scripts/test.sh
```

### 预期现象

`exercise-printcolor` 输出包含：

```text
Hello, Arceos!
✓ colored output detected
✓ All exercise tests passed!
```

`exercise-hashmap` 输出包含：

```text
test_hashmap() OK!
Memory tests run OK!
✓ All exercise tests passed!
```

`exercise-altalloc` 输出包含：

```text
Running bump tests...
Bump tests run OK!
✓ All exercise tests passed!
```

`exercise-ramfs-rename` 输出包含：

```text
[Ramfs-Rename]: ok!
✓ All exercise tests passed!
```

`exercise-sysmap` 输出包含：

```text
Read back content: hello, arceos!
MapFile ok!
✓ All exercise tests passed!
```
