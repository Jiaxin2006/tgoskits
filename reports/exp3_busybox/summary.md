# Exp3 Report: BusyBox 兼容性测试

> 对应 PR：[#349](https://github.com/rcore-os/tgoskits/pull/349)  
> 展示定位：把 StarryOS 兼容性从单点 syscall 推进到真实 BusyBox applet。

## 1. 实验目标

实验三关注 BusyBox 兼容性。相比实验二直接测试 syscall 边界，BusyBox 的入口更接近真实应用：

```text
BusyBox applet
  -> libc / shell
  -> syscall / procfs / VFS / fd / 环境变量
  -> StarryOS kernel
```

PR #349 的主要目标是恢复第一批对 PostgreSQL bring-up 有用的 BusyBox applet 检查，并把 QEMU success matcher 收紧到必须出现 `FAIL: 0`。

## 2. PR 内容

PR #349 标题为：

```text
test(starryos): expand riscv64 busybox coverage
```

它是 test coverage only，新增或恢复了这些 applet 检查：

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

同时修改 `qemu-riscv64.toml`，让成功条件要求 `FAIL: 0`，避免测试脚本中某些 PASS 行被误匹配造成假绿。

## 3. 为什么 BusyBox 测试重要

单个 syscall 测试能固定精确语义，但真实应用经常组合很多接口。以 BusyBox 为例：

- `env` 依赖环境变量和 PATH；
- `link` 依赖 hard link / VFS 语义；
- `mkdir`、`rmdir`、`mv` 依赖目录操作和 errno；
- `tar`、`cpio` 依赖文件读写、元数据、路径处理；
- `tail`、`split`、`xxd` 依赖稳定文件 I/O。

因此 BusyBox 测试是应用级 oracle。它不一定直接告诉我们哪个 syscall 错了，但能暴露真实用户程序会遇到的组合问题。

## 4. Test Harness 改进

### 4.1 收紧 success matcher

旧测试如果只匹配某个 PASS 字符串，可能出现这种情况：

```text
PASS: 200
FAIL: 3
```

但 QEMU harness 仍然误判成功。

PR #349 把成功条件收紧到必须要求：

```text
FAIL: 0
```

这让 BusyBox case 更像真正的测试套件，而不是只检查“程序曾经输出过一些成功文本”。

### 4.2 输出失败命令

PR 中还增强了脚本诊断能力：当某个命令失败时，打印失败命令输出，方便后续把 applet 失败追到底层 syscall 或 VFS 问题。

## 5. 与实验二的关系

实验二解决的是精确 syscall 语义；实验三把这些语义放进真实应用环境中验证。

例如：

- `link` applet 会触发 hard link 语义，与 PR #348 的问题相关；
- `mkdir`、`mv`、`rmdir` 会压力测试路径解析和目录项操作；
- `tar`、`cpio` 会组合测试文件内容、元数据和路径处理。

所以 BusyBox 不是“另一组普通测试”，而是把 syscall/VFS 修复推向应用兼容面的桥梁。

## 6. 学到的 OS 点

1. **真实应用依赖接口组合。** 一个 applet 失败，背后可能是 syscall、VFS、procfs、环境变量或脚本行为。
2. **应用级 oracle 容易假绿。** 必须要求 `FAIL: 0` 或明确 marker。
3. **测试 PR 也有价值。** 没有长期覆盖，后续兼容性修复很容易退化。
4. **stdout/stderr 也是可观察行为。** 很多 shell 工具依赖文本输出和退出码，不能只看内核返回值。

