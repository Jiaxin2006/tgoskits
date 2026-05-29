# Big Lab B 期末展示报告

> 韩佳辛  
> 日期：2026-05-29  
> 展示主线：从 ArceOS 入门，到 StarryOS Linux 兼容性，再到 backtrace 诊断能力；重点回答“我通过大实验学到了哪些新的 OS 东西”。

## 一、整体概述

本次 biglab B 的材料可以分成两层：

- **Task1：ArceOS tutorial**  
  通过 `tg-arceos-tutorial` 中的教学实验熟悉 ArceOS/TGOSKits 的构建、运行、测试方式，并建立 `no_std`、VFS、内存分配、用户地址空间和 syscall 的基础理解。

- **Task2：StarryOS/TGOSKits 实验**  
  包括 polyglot harness、StarryOS syscall/文件系统兼容性 PR、BusyBox 测试覆盖、Issue #146 backtrace 重做。

这组实验不是互不相关的任务，而是形成一条逐步深入的主线：

```text
ArceOS 教学实验建立系统分层意识
  -> polyglot harness 固定 AI 协作与验证纪律
  -> syscall / VFS PR 修复 Linux ABI 细节
  -> BusyBox 测试把兼容性推进到真实应用层
  -> backtrace 重做提升 OS 调试和诊断能力
```

从展示角度，我不想把它讲成“做了多少 PR”，而是讲成：这些任务如何让我从“能跑代码”走向“能解释 OS 机制”。

本目录中的子报告与 demo：

- `task1_tutorial/report/summary.md`
- `task1_tutorial/demo/demo.md`
- `exp1_pipeline/report/summary.md`
- `exp1_pipeline/demo/demo.md`
- `exp2_syscall/report/summary.md`
- `exp2_syscall/demo/demo.md`
- `exp3_busybox/report/summary.md`
- `exp3_busybox/demo/demo.md`
- `exp4_backtrace/report/summary.md`
- `exp4_backtrace/demo/demo.md`

## 二、Task1：从 ArceOS 教学实验建立系统分层意识

Task1 的仓库是 [Jiaxin2006/tg-arceos-tutorial](https://github.com/Jiaxin2006/tg-arceos-tutorial)。它包含多个 ArceOS 教学 crate，既有 `app-*` 示例，也有 `exercise-*` 练习。

我重点理解了五个练习：

| 练习 | 主题 | 学到的 OS 概念 |
| --- | --- | --- |
| `exercise-printcolor` | 彩色输出 | bare-metal 输出链路、QEMU serial |
| `exercise-hashmap` | `HashMap` 支持 | `no_std + alloc`、Cargo feature |
| `exercise-altalloc` | bump allocator | 分配器、对齐、释放策略 |
| `exercise-ramfs-rename` | ramfs rename | VFS 分发、目录项、锁 |
| `exercise-sysmap` | `mmap` syscall | 用户地址空间、页映射、文件映射 |

### 2.1 `ramfs-rename`: VFS 分发比单点实现重要

`std::fs::rename` 的实际路径是：

```text
std::fs::rename
  -> axstd
  -> axfs root API
  -> RootDirectory
  -> axfs_ramfs::DirNode
```

只在 `DirNode` 中实现 rename 不够。如果 `RootDirectory` 不把请求转发到实际挂载文件系统，用户调用仍然会返回 `Unsupported`。

这个实验让我认识到，文件系统 bug 经常不是“某个函数没实现”这么简单，而是 VFS 分发层和具体文件系统实现层没有接起来。

另外，rename 的本质是目录项操作，不是复制文件内容。它改变的是目录中的 name -> node 绑定，文件对象本身不应被复制。

### 2.2 `sys_mmap`: 用户态地址不是普通指针

`exercise-sysmap` 中实现 `sys_mmap`，核心流程是：

```text
用户态 mmap
  -> syscall trap
  -> 内核校验参数
  -> USER_ASPACE 建立页映射
  -> 文件内容写入映射地址
  -> 返回用户虚拟地址
```

这个实验让我理解到，用户传入的地址、长度、权限、fd 都必须经过内核校验。`mmap` 返回的不是一个普通 Rust 指针，而是用户虚拟地址空间中的一段有效 mapping。

这些经验后来直接迁移到 StarryOS 的 mmap、用户指针 copyin/copyout、poll/epoll 等兼容性问题。

### 2.3 `altalloc`: 分配器设计是取舍

`exercise-altalloc` 的 bump allocator 使用双端布局：

```text
[ byte used -> | free space | <- page used ]
```

它分配快、状态少，但不能精细释放，适合 boot/early allocator，不适合作为长期通用 allocator。

这个实验让我第一次清楚地看到 allocator 的设计目标和场景相关，不能用一个抽象 API 掩盖底层取舍。

## 三、Exp1：polyglot-agent-harness 与 AI 协作纪律

实验一对应 `polyglot-agent-harness`。它的目标不是让 AI 偶尔写出代码，而是建立一个可复现、可验证、可审查的协作流程。

关键组件包括：

- `AGENTS.md`：统一工作规则；
- `skills/*/SKILL.md`：可复用流程，例如 `report-generator`、`pr-workflow`、`experiment-guard`；
- `scripts/docker-check.py`：Docker preflight；
- `pr-drafts/`：本地 PR 草稿；
- 报告制度：bug、失败、PR 准备都要有证据记录。

我理解它的核心价值是：**把 AI 的实现能力放进工程纪律里。**

它不只是多个 skill 的简单集合。`AGENTS.md` 负责判断什么时候必须触发某个 skill，`skills/*/SKILL.md` 负责把经验写成 checklist，`scripts/docker-check.py` 负责可执行的环境预检，`reports-local/` 与 `pr-drafts/` 负责把实验过程变成证据。

展示时最适合具体展开三个 skill：

- `experiment-guard`：同步 upstream、merge/rebase 或开始高风险实验前使用，要求 fetch、确认 base、冲突停止、Docker preflight 和验证记录。
- `test-authoring`：写测试前使用，要求选对 Rust/C/script 落点，补齐 `build-*.toml` / `qemu-*.toml`，用 success/fail regex 防假绿。
- `report-generator`：失败、bug 修复或 PR 准备时使用，要求记录现象、根因、修复、验证和风险。

本地 Codex session 记录和 harness git history 能还原它的改进过程：先通过工作区 `AGENTS.md` 让 agent 自动读取本地 skills，再把 Docker 默认策略、结构化报告、PR 草稿、完成度复核和 reusable container/cache 逐步写回 harness。比如 `54bba1c harness: document reusable docker verification` 就来自 backtrace/memtrack 验证中发现 `docker run --rm` 会反复丢 rustup 状态。

AI 适合做：

- 阅读代码；
- 整理调用链；
- 生成初版补丁；
- 总结报告。

但规则和人必须负责：

- 是否同步 upstream；
- 是否在 Docker/CI-like 环境验证；
- 测试是否真的覆盖语义；
- PR 范围是否过大；
- 是否记录失败和限制。

这部分对 OS 实验很重要，因为 OS 问题往往跨工具链、QEMU、rootfs、测试脚本和内核状态。没有 workflow，很容易得到一个“这次能跑，但无法复现”的结果。

## 四、Exp2：StarryOS Syscall 与 VFS 兼容性

实验二集中在 StarryOS 的 Linux 兼容性边界。我的相关 PR 包括：

| PR | 主题 | 核心问题 |
| --- | --- | --- |
| [#265](https://github.com/rcore-os/tgoskits/pull/265) | `unlinkat` invalid flags | 非法参数必须在副作用前拒绝 |
| [#348](https://github.com/rcore-os/tgoskits/pull/348) | tmpfs hardlink user data | hard link 必须共享数据状态 |
| [#350](https://github.com/rcore-os/tgoskits/pull/350) | syscall boundary tests | 错误优先级和边界行为是 ABI |

### 4.1 `unlinkat`: 错误路径不能有副作用

PR #265 修复 `unlinkat` 没有校验非法 flags 的问题。

Linux 语义中，`unlinkat` 只接受 `AT_REMOVEDIR`。旧实现把其他 bit 静默忽略，甚至可能删除文件。

修复思路是：

```text
if flags & !AT_REMOVEDIR != 0:
    return EINVAL

if flags & AT_REMOVEDIR != 0:
    remove_dir(path)
else:
    remove_file(path)
```

这个 PR 给我的最大收获是：Linux ABI 不只是成功路径。错误路径也必须无副作用。非法参数不能先删除文件，再说调用失败。

### 4.2 tmpfs hardlink: stat 正确不代表数据路径正确

PR #348 修复 tmpfs hard link 新路径读出零填充的问题。

问题表现是：新路径的 inode、nlink、size 看起来正确，但实际读内容错误。

根因是 tmpfs 文件内容保存在 axfs-ng page cache user data 中。hard link 创建了新的 `DirEntry`，但没有继承源 entry 的 user data。

这个案例让我理解到：

```text
hard link = 多个目录项共享同一个文件对象/数据状态
```

文件系统正确性不能只看元数据，还必须检查实际数据路径。

### 4.3 syscall boundary tests: 错误优先级也是 ABI

PR #350 增强了 `getrandom`、`prlimit64`、`getrlimit`、`setrlimit` 的边界测试。

例如 `getrandom` 中：

- `len == 0` 应返回 0，且不访问用户 buffer；
- invalid flags 应返回 `EINVAL`；
- bad pointer 在非零长度时返回 `EFAULT`；
- invalid flags 和 bad pointer 同时出现时，应该优先返回 `EINVAL`。

这说明 Linux ABI 中有很多组合输入的错误优先级。应用和测试可以观察这些细节，因此内核实现不能只处理常见成功路径。

## 五、Exp3：BusyBox 兼容性测试

实验三对应 [PR #349](https://github.com/rcore-os/tgoskits/pull/349)，目标是扩展 StarryOS riscv64 BusyBox 测试覆盖。

恢复的 applet 包括：

```text
chown, cpio, dos2unix, env, link, mkdir, mv,
rmdir, split, tail, tar, xxd
```

这个 PR 是 test coverage only，但它很重要，因为 BusyBox 是应用级 oracle。它不是直接问某个 syscall 是否返回某个值，而是让真实小工具组合使用 syscall、VFS、环境变量、文件内容和 stdout/stderr。

PR 中还把 success matcher 收紧到要求：

```text
FAIL: 0
```

这是为了避免假绿。否则脚本中某个子项输出 PASS，也可能让整个 QEMU case 被误判为成功。

我从这里学到：真实应用兼容性不是“实现 syscall 表”这么简单。一个 applet 背后可能依赖：

- 文件系统路径解析；
- hard link；
- 目录操作；
- 环境变量；
- 文件元数据；
- stdout/stderr 文本格式；
- shell 退出码。

BusyBox 测试把实验二的 syscall/VFS 修复推向了真实应用层。

## 六、Exp4：Backtrace 诊断能力重做

实验四围绕 issue #146，对 backtrace 进行重构。本地报告是 `issue-146-backtrace-progress.md`。

相关 PR（全部已合并入上游 `rcore-os/tgoskits`）：

| PR | 核心内容 |
| --- | --- |
| [#619](https://github.com/rcore-os/tgoskits/pull/619) | axbacktrace 重构：不依赖 DWARF 也能输出 raw `ip/fp` block |
| [#635](https://github.com/rcore-os/tgoskits/pull/635) | host symbolizer 鲁棒性：防止前缀噪声/缺 END/假绿 |
| [#646](https://github.com/rcore-os/tgoskits/pull/646) | 三个边界 QEMU 用例：basic / normal / badfp |
| [#748](https://github.com/rcore-os/tgoskits/pull/748) | 统一输出格式：`Backtrace::kind()` 链式 API |
| [#749](https://github.com/rcore-os/tgoskits/pull/749) | ArceOS QEMU 测试后自动调用 host symbolizer |
| [#793](https://github.com/rcore-os/tgoskits/pull/793) | raw block 闭合后流式打印符号化结果 |
| [#1023](https://github.com/rcore-os/tgoskits/pull/1023) | **bug 修复**：`--config target.<key>.rustflags` 因 #839 静默失效，导致四架构帧指针丢失 |
| [#1020](https://github.com/rcore-os/tgoskits/pull/1020) | StarryOS `/dev/memtrack` 接入 raw block / host symbolize，E2E 覆盖 alloc backtrace 链路 |

旧问题是：backtrace 把调用栈展开和 DWARF 符号化强耦合。不开 `dwarf` 时，raw backtrace 也像被禁用；panic/trap 路径中做复杂 DWARF 解析也不够 robust。

我采用的设计是：

```text
target 侧：
  稳定输出 raw backtrace block

host 侧：
  读取 raw block + ELF
  用 addr2line 符号化
```

### 6.1 target raw block

target 输出格式类似：

```text
BACKTRACE_BEGIN kind=raw arch=x86_64 alloc=true dwarf=true
BT 0 ip=0x... fp=0x...
BT 1 ip=0x... fp=0x...
BACKTRACE_END
```

这里 raw block 只有地址，没有函数名。

### 6.2 host-side symbolize

host 用同次构建 ELF 做符号化：

```bash
cargo xtask backtrace symbolize \
  --elf target/x86_64-unknown-none/release/arceos-backtrace-raw-normal \
  --log /tmp/arceos-backtrace-e2e.log \
  --kind raw
```

自动路径中，QEMU raw block 闭合后可以直接在终端看到：

```text
=== host backtrace symbolize ===
```

### 6.3 为什么需要 FP

当前 unwind 依赖 frame pointer 链：

```text
fp -> previous fp + saved return address ip
```

只有 IP 不够，因为 IP 只说明这一帧在哪里，不能告诉我们上一帧在哪里。因此构建链路必须保留 FP：

```text
-C force-frame-pointers=yes
```

### 6.4 为什么 symbolize 放 host

panic/trap 时 target 可能已经处于不稳定状态。如果在 target 上解析 DWARF，需要访问 `.debug_*` 段、跑 `addr2line/gimli`、处理更复杂的数据结构，失败模式更多。

host-side 更稳：

- host 有完整 ELF 和工具链；
- 解析失败不会让 guest 再 panic；
- CI 可以先验证 raw unwind；
- target 只需要输出可解析日志。

这个实验让我理解到，调试设施本身也是 OS 架构设计的一部分。robust 的诊断能力需要把高风险路径做简单，把复杂分析放到更稳定的一侧。

### 6.5 一次隐蔽的构建回归：帧指针 flag 被静默丢弃（PR #1023）

实验中期发现一个非常典型的"静默降级"案例。

PR #839（`refactor(axbuild): use target JSON specs for kernel builds`）把内核构建从内置 target triple 改为自定义 JSON target spec 文件后，注入帧指针的 `--config target.<key>.rustflags` 的 key 写成了 spec 文件**路径**（`target.'scripts/.../x86_64-unknown-none.json'.rustflags`）；但 cargo 对 JSON target 是按文件 **stem** 匹配，路径 key 永远对不上，于是 `-C force-frame-pointers=yes` 被 cargo 静默丢弃——没有任何报错。

症状：`BACKTRACE=y` 的用例 raw backtrace 退化为单帧 `BT 0 ip=0x4 fp=0xffffffffffffffff`，四架构全部受影响。

定位方式：`cargo build --verbose | grep force-frame-pointers`，命中数为 0，证明 flag 根本没进 rustc。

修复（PR #1023）：一处 `format!`，从 spec 文件 stem 派生 config key，恢复全架构多帧展开。

这个案例的价值：**构建系统中传错 key 往往不报错，而是静默降级，症状和根因之间距离很远**。必须用 `--verbose` 落到真实 rustc 命令行才能确认。

### 6.6 memtrack backtrace E2E（PR #1020）

> 助教确认 alloc tracking 的真实 caller stack 可以不作为本轮硬性目标。这里是超出要求的额外实现，目的是验证框架对真实场景的通用性，以及通过 `sample_hard` 探针暴露并帮助定位了 §6.5 的帧指针回归。

在 arceos backtrace 框架稳定后，将同一套 raw block / host symbolize 链路接入 StarryOS `/dev/memtrack`，为 `ax-alloc/tracking` 记录的 allocation backtrace 补上端到端测试。

核心变化：

- `starry-kernel/memtrack` 依赖从 `gimli`（target-side DWARF）改为 `ax-feat/backtrace + ax-alloc/tracking`，不再强制要求 `DWARF=y`。
- `/dev/memtrack` 新增三个调试命令：`sample\n`（稳定制造 4096 字节 allocation）、`sample_hard\n`（深调用链：`mid → leaf → Vec::with_capacity(8192)`，用于观察真实 alloc unwind）、`symbolize\n`（确定性单帧 probe，验证 host symbolize 链路）。
- `axbuild` Starry test runner 支持 `host_symbolize_success_regex`，允许 E2E 断言 host 侧函数名输出。
- 新增 `test-suit/starryos/normal/memtrack-backtrace`，在 x86_64 QEMU 中验证 guest raw alloc block 和 host symbolization。

E2E 通过标准（CI 已验证）：

```text
STARRY_MEMTRACK_BACKTRACE_OK
=== host backtrace symbolize ===
BT 0 ip=... fp=0x0 starry_memtrack_symbolize_probe
result: 1/1 case(s) passed
```

在帧指针修复（#1023）后，`sample_hard` 能稳定展开真实 alloc 调用链：

```text
BT 0  starry_memtrack_sample_hard_leaf
BT 1  starry_memtrack_sample_hard_mid
BT 2  record_hard_sample_allocation
BT 3  MemTrack::write (DeviceOps)
...   ax_fs_ng / sys_writev / handle_syscall / task_entry
```

这个测试的价值不只是覆盖 memtrack，还说明 alloc tracking 可以在不修改任何业务代码的情况下，把一笔堆分配的完整调用链记录下来——对诊断内存泄漏或异常分配很有价值。

## 七、StarryOS 架构分析

结合这些实验，我对 StarryOS 的架构理解可以概括成四层：

```text
Linux 应用
  -> Linux ABI / syscall wrapper
  -> StarryOS kernel modules
  -> ArceOS components
  -> platform / hardware / QEMU
```

### 7.1 Linux ABI / syscall wrapper

用户态通过 syscall 进入内核，StarryOS 从 `UserContext` 读取参数并分发到具体 `sys_*` 实现。

这一层需要处理：

- flags/mode/fd 校验；
- 用户指针 copyin/copyout；
- Linux errno 映射；
- 错误优先级；
- 副作用发生时机。

PR #265 和 #350 都属于这一层。

### 7.2 VFS / fd / FileLike

StarryOS 中普通文件、目录、socket、pipe、epoll 等对象都通过 fd 暴露。统一 fd 抽象符合 Linux “一切皆文件”的风格，也让 poll/epoll、close、cloexec 等机制能复用。

但这也带来风险：不同 fd 类型在不同 syscall 下的 Linux 错误语义不同，不能简单把所有对象当成同一种文件。

PR #348 和 BusyBox 测试都体现了这一层的重要性。

### 7.3 pseudofs / procfs / devfs

StarryOS 中很多路径并不是普通磁盘文件，而是内核暴露给用户态的接口。例如 `/dev/memtrack`：

```text
用户 echo start > /dev/memtrack
  -> write syscall
  -> VFS 找到 pseudofs device node
  -> DeviceOps::write_at
  -> 内核开启 allocation tracking
```

这仍然是 target 内核态代码，不是进入另一个硬件设备程序，也不是 S 态到 M 态切换。

### 7.4 task / process / signal

真实 Linux 应用还依赖 PID、进程组、session、signal、wait、zombie 等状态。即使我的 PR 没有集中修 task 子系统，BusyBox 和 Codex 类实验都说明，进程状态不是 alive/dead 二值。

未来 StarryOS 要运行更多真实应用，必须系统化维护用户可见进程状态。

## 八、我学到的 OS 知识案例

### 8.1 错误路径也是 ABI

`unlinkat` 非法 flags 不能删除文件；`getrandom` invalid flags 和 bad pointer 同时出现时，返回哪个 errno 也是用户可见行为。

### 8.2 VFS bug 要沿调用链看

`ramfs-rename` 说明只改具体文件系统节点不够，还要看上层 root/VFS 是否分发到它。

### 8.3 文件系统正确性包含数据路径

`tmpfs hardlink` 说明 inode/nlink/size 正确不代表 read 数据正确。page cache/user data 也是文件系统语义的一部分。

### 8.4 应用测试能暴露组合问题

BusyBox applet 背后组合了多个内核接口。单点 syscall 测试通过，不代表真实应用通过。

### 8.5 backtrace 是 unwind + symbolize

我以前容易把“打印调用栈”看成一个整体。现在能区分：

- FP 链负责 unwind；
- IP 是 raw 地址；
- DWARF/ELF/addr2line 负责 symbolize；
- target/host 划分决定诊断路径是否 robust。

## 九、对 AI 协作的反思

AI 在这次实验中很有帮助，但它不能替代 OS 学习。

AI 擅长：

- 搜索调用链；
- 生成初版实现；
- 补测试；
- 总结报告；
- 对比相似代码模式。

但人必须负责：

- 判断 Linux 语义是否正确；
- 识别测试是否假绿；
- 决定 PR 粒度；
- 解释为什么这样设计；
- 在答辩中讲清每个机制。

所以我对 AI 协作的理解是：

```text
AI 降低了完成代码的难度，
但提高了“什么才算真正理解”的标准。
```

## 十、对未来 biglab B 和 OS 教学的展望

未来 biglab B 可以继续采用真实应用驱动的方向，但评价重点应从“代码能跑”转向“机制能解释”。

我建议保留几个要求：

1. **Linux oracle**  
   每个兼容性修复要说明 Linux 在同样输入下的行为。

2. **最小复现**  
   真实应用失败后，要抽出最小 C regression 或脚本 regression。

3. **长期测试**  
   修复必须进入 test-suit 或等价长期验证入口。

4. **架构解释**  
   学生要能说明 bug 穿过了哪些 OS 层，例如 syscall、VFS、fd table、mm、task、net。

5. **AI 使用记录**  
   可以使用 AI，但要记录问题如何提出、AI 给了什么方案、人如何验证、哪里是人工判断。

对于 OS 教学，我认为未来最重要的不是禁止 AI，而是设计 AI 不能轻易替代的学习环节：

- 手画一条调用链；
- 解释一个 errno 为什么应该优先返回；
- 说明用户指针什么时候可以访问、什么时候不能持有；
- 讲清一个真实 bug 的现象、根因、修复和测试；
- 在答辩中追问 PPT 上每个实现细节。

## 十一、最终总结

这次 biglab B 对我的最大价值不是“完成了几个任务”，而是让我真正把几个 OS 概念串了起来：

```text
VFS 分发
用户地址空间
分配器
Linux syscall ABI
文件系统数据一致性
应用级兼容性
backtrace 调试能力
```

如果用一句话总结：

```text
代码结果可以由 AI 协作完成，
但机制理解、系统判断和调试能力必须由自己建立。
```

这也是我期末展示想传达的核心。
