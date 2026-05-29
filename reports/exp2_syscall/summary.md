# Exp2 Report: StarryOS Syscall 与边界语义

> 对应 PR：[#265](https://github.com/rcore-os/tgoskits/pull/265)、[#348](https://github.com/rcore-os/tgoskits/pull/348)、[#350](https://github.com/rcore-os/tgoskits/pull/350)  
> 本地补充材料：`STARRY_SYSCALL_BOUNDARY_TESTS.md`、`tgoskits/reports/syscall_preadv_pwritev2_modification_report.md`  
> 展示定位：说明 Linux 兼容性不是 syscall 名字，而是用户态可观察语义。

## 1. 实验目标

实验二围绕 StarryOS 的 Linux 兼容性边界展开。我的几个 PR 共同体现一个原则：

```text
通过 Linux 行为基准和 StarryOS 回归测试，
把小而精确的 ABI 差异修成可维护的上游补丁。
```

这类任务的难点不在“有没有 syscall”，而在：

- flags 是否严格校验；
- errno 是否符合 Linux；
- 错误路径是否无副作用；
- 文件系统元数据和数据路径是否一致；
- regression 是否能防止以后退化。

## 2. PR 总览

| PR | 状态 | 主题 | 主要 OS 点 |
| --- | --- | --- | --- |
| [#265](https://github.com/rcore-os/tgoskits/pull/265) | merged | `unlinkat` invalid flags | syscall 参数校验、错误路径无副作用 |
| [#348](https://github.com/rcore-os/tgoskits/pull/348) | merged | tmpfs hardlink user data | VFS、hard link、page cache user data |
| [#350](https://github.com/rcore-os/tgoskits/pull/350) | merged | `getrandom` / `prlimit64` 边界测试 | Linux ABI、错误优先级、测试覆盖 |

## 3. PR #265: `unlinkat` invalid flags

### 3.1 问题现象

旧实现没有拒绝 `unlinkat` 的非法 `flags`。除 `AT_REMOVEDIR` 以外的 bit 被静默忽略，导致：

- `unlinkat(..., flags=0x1)` 本应返回 `EINVAL`，实际可能删除文件；
- `AT_SYMLINK_NOFOLLOW` 对 `unlinkat` 无意义，本应返回 `EINVAL`；
- `AT_REMOVEDIR | 0x1` 本应返回 `EINVAL`，旧逻辑可能走到错误分支。

### 3.2 根因

旧逻辑类似：

```text
if flags == AT_REMOVEDIR:
    remove_dir(path)
else:
    remove_file(path)
```

它有两个问题：

- 没有检查 `flags & !AT_REMOVEDIR`；
- 用 `==` 判断目录删除，而不是基于 bit 判断。

### 3.3 修复设计

修复后的语义是：

```text
if flags & !AT_REMOVEDIR != 0:
    return EINVAL

if flags & AT_REMOVEDIR != 0:
    remove_dir(path)
else:
    remove_file(path)
```

这保证非法参数在路径删除前就被拒绝。

### 3.4 测试

新增 `bug-unlinkat-einval` QEMU 用例，覆盖：

- file + invalid flags 返回 `EINVAL`，文件仍存在；
- file + `AT_SYMLINK_NOFOLLOW` 返回 `EINVAL`；
- file + flags=0 正常删除；
- dir + `AT_REMOVEDIR | invalid` 返回 `EINVAL`，目录仍存在；
- dir + `AT_REMOVEDIR` 正常删除。

PR 中记录四架构 QEMU 通过：riscv64、aarch64、x86_64、loongarch64。

### 3.5 学到的 OS 点

这个 PR 最重要的结论是：**错误路径也是 ABI**。非法 flags 不能有副作用，不能先删除文件再返回错误，也不能静默忽略。

## 4. PR #348: tmpfs hardlink cache

### 4.1 问题现象

tmpfs 创建 hard link 后，新路径看到的 inode、nlink、size 是对的，但读取内容是 0 填充。

这说明元数据路径和数据路径不一致：

```text
stat 看起来正确
read 实际数据错误
```

### 4.2 根因

StarryOS tmpfs 的 regular file 内容存放在 axfs-ng page cache 的 user data 中。hard link 创建了第二个 `DirEntry` 指向同一个 inode，但没有继承源 entry 的 user data。

结果是：

- 新路径共享了 inode 元数据；
- 新路径没有共享 page cache/user data；
- 读取时看到的是空页或零填充。

### 4.3 修复设计

hard link 创建新 `DirEntry` 时，要保留源 entry 的 user data，使两个目录项共享同一份文件内容状态。

这符合 hard link 的核心语义：

```text
多个目录项 -> 同一个文件对象 / inode / data
```

### 4.4 测试

新增 `bug-tmpfs-hardlink-cache` 回归测试，检查：

- hard link 后 nlink/size 正确；
- 通过新路径读出的内容与源路径一致；
- 多架构 QEMU 配置覆盖 x86_64、riscv64、aarch64、loongarch64。

### 4.5 学到的 OS 点

文件系统 bug 不能只看 `stat`。文件大小、link count 和 inode 正确，不代表数据缓存路径正确。hard link 的正确性必须检查实际读写内容。

## 5. PR #350: syscall boundary tests

### 5.1 目标

PR #350 主要增强 `getrandom`、`getrlimit`、`setrlimit`、`prlimit64` 的边界测试，并用文档记录 Linux 可观察行为。

这类测试的目标不是覆盖 happy path，而是固定容易回归的 ABI 细节。

### 5.2 `getrandom` 覆盖点

测试覆盖：

- 正常读取返回请求长度；
- `len == 0` 返回 0，并且不访问用户 buffer；
- valid flag combinations；
- unknown flags 返回 `EINVAL`；
- `GRND_RANDOM | GRND_INSECURE` 互斥组合返回 `EINVAL`；
- bad user pointer 在非零长度时返回 `EFAULT`；
- invalid flags 与 bad pointer 同时出现时，应优先返回 `EINVAL`；
- 4096-byte 大请求能完整返回。

### 5.3 `prlimit64` 覆盖点

测试覆盖：

- `RLIMIT_NOFILE`、`RLIMIT_STACK` 基础读取；
- invalid resource 返回 `EINVAL`；
- bad user pointer 返回 `EFAULT`；
- `rlim_cur > rlim_max` 返回 `EINVAL`；
- 降低 soft/hard limit 后能被后续 `getrlimit` 观察；
- 恢复 hard limit 时不是 silent no-op。

### 5.4 学到的 OS 点

Linux ABI 中最容易被忽略的是错误优先级。比如 invalid flags 和 bad pointer 同时出现时，先返回 `EINVAL` 还是 `EFAULT` 是用户态可观察的行为。测试必须覆盖这种组合输入。

## 6. 共性总结

实验二的几个 PR 共同说明：

1. **参数校验必须在副作用前发生。**  
   `unlinkat` 非法 flags 不能删除文件。

2. **文件系统元数据和数据缓存都要正确。**  
   hard link 不能只共享 inode，还要共享实际内容状态。

3. **测试应该覆盖错误路径。**  
   syscall 成功路径通过不代表兼容性正确。

4. **Linux 行为要用最小 regression 固化。**  
   否则后续重构很容易让细节退化。
