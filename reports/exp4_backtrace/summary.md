# Exp4 Report: Issue #146 Backtrace 重做

> 相关 issue：<https://github.com/rcore-os/tgoskits/issues/146>  
> 展示定位：把 backtrace 拆成 target raw unwind 与 host-side symbolize，提升 panic/trap 诊断鲁棒性。

## 1. 实验目标

实验四围绕 backtrace 诊断能力展开。旧实现的问题是：调用栈展开和 DWARF 符号化强耦合，导致不开 `dwarf` 时 raw backtrace 也像被禁用；同时 panic/trap 等高风险路径中做复杂 DWARF 解析并不稳健。

目标设计是：

```text
target 侧：稳定输出 raw backtrace block
host 侧：读取 raw block + ELF，用 addr2line 符号化
```

这把 backtrace 拆成两个概念：

- **unwind**：从当前 frame pointer 链得到一串 `ip/fp` 地址；
- **symbolize**：把 `ip` 地址映射到函数名、文件名、行号。

## 2. 背景问题

旧实现存在几类问题：

1. **不开 DWARF 时 backtrace 像被禁用。**  
   `capture()` / `capture_trap()` 在未开启 `dwarf` 时可能返回 disabled，无法得到 raw 地址序列。

2. **构建链路没有稳定保留 frame pointer。**  
   raw unwind 依赖 FP 链。如果 xtask/QEMU 路径没有注入 `-C force-frame-pointers=yes`，就可能出现 `BT 0 ip=0x0 fp=0x0` 或 invalid fp。

3. **host 侧解析不够鲁棒。**  
   日志里有 `BACKTRACE_*` 块，但工具可能因前缀噪声、缺 END、重复 BEGIN 等情况解析失败。

4. **QEMU 测试可能假绿。**  
   如果 success regex 多条是 OR 语义，只匹配到 `BACKTRACE_BEGIN` 就通过，不能证明真的 unwind 成功。

## 3. 核心设计

### 3.1 target-side raw block

target 侧输出固定格式：

```text
BACKTRACE_BEGIN kind=<kind> arch=<arch> alloc=<bool> dwarf=<bool>
BT 0 ip=0x... fp=0x...
BT 1 ip=0x... fp=0x...
BT_ERROR <reason>
BACKTRACE_END
```

常见 `kind`：

- `raw`：普通测试或显式捕获；
- `panic`：panic handler；
- `trap`：trap/exception；
- `alloc`：memtrack/alloc tracking 等诊断路径。

raw block 的优势是：

- 格式固定；
- 可被 CI 和 host 工具解析；
- 不依赖目标侧完整 DWARF 符号化；
- 即使失败也能用 `BT_ERROR` 结构化表达原因。

### 3.2 host-side symbolize

host 侧工具读取 raw block，使用同次构建 ELF 和 `addr2line` 做符号化：

```bash
cargo xtask backtrace symbolize \
  --elf target/x86_64-unknown-none/release/arceos-backtrace-raw-normal \
  --log /tmp/arceos-backtrace-e2e.log \
  --kind raw
```

自动路径中，`cargo xtask arceos test qemu` 可以在 QEMU 输出 raw block 闭合后流式打印：

```text
=== host backtrace symbolize ===
...
```

这样终端看到的是 demangled 栈，而不是原始 `BT ip/fp` 行。

### 3.3 为什么 symbolize 放 host

target-side DWARF 需要解析 `.debug_*` 段、初始化 `addr2line/gimli`，panic/trap 时依赖链较长，失败模式更多。

host-side 更安全：

- host 有完整 ELF、工具链和内存；
- guest 崩溃后不会因为符号化再触发二次 panic；
- 解析失败只影响 host 工具；
- CI 可先断言 raw unwind 是否成功，再单独验证 symbolize。

这体现了一个 OS 诊断设计原则：**高风险 target 路径保持简单，把复杂分析移到 host。**

## 4. Frame Pointer 与 DWARF

raw unwind 依赖 frame pointer：

```text
当前 fp -> 上一帧 fp + 返回地址 ip
       -> 再上一帧 fp + 返回地址 ip
```

`ip` 只能说明“这一帧在哪里”，不能告诉我们上一帧在哪里；`fp` 才是沿栈向上走的索引。因此构建时必须保留 FP：

```text
-C force-frame-pointers=yes
```

DWARF 不是 unwind 的唯一前提。在当前设计中：

- `BACKTRACE=y` / frame pointer 负责可 unwind；
- `DWARF=y` / debuginfo 负责可符号化；
- `DWARF=y` 可以隐式开启 backtrace 所需构建条件，但 raw unwind 不应反过来依赖 target-side DWARF。

### 4.1 一个隐蔽的构建回归：帧指针 flag 被静默丢弃

实验过程中遇到一次很有代表性的回归：明明 build config 开了 `BACKTRACE=y`，
raw backtrace 却退化成只能展开一帧（`BT 0 ip=0x4 fp=0xffffffffffffffff`），
四个架构原本正常的多层展开全部消失。

根因是 PR #839（`refactor(axbuild): use target JSON specs for kernel builds`）：

- #839 把内核构建从内置 target triple 改为自定义 **JSON target spec** 文件；
- per-target 的帧指针 flag 通过 `cargo --config target.<key>.rustflags=...` 注入；
- 改动后 `<key>` 写成了 spec 文件**路径**（`target.'scripts/.../x86_64-unknown-none.json'`）；
- 但 cargo 对 JSON target 是按文件 **stem**（`x86_64-unknown-none`）匹配 `target.<name>`，
  路径 key 永远匹配不上 → `-C force-frame-pointers=yes` 被**静默丢弃**，没有任何报错。

验证方式：`cargo build --verbose` grep `force-frame-pointers`，命中数为 0，
说明 flag 根本没到 rustc。

修复（`fix/axbuild: use target spec stem as rustflags config key`）只需让 config key
从 spec 文件 stem 派生（恰好等于原 triple），并且只在 `extra_rustflags` 非空时注入，
非 backtrace 用例不受影响、不付额外寄存器代价。修复后四架构 raw unwind 全部恢复。

这条经验值得记住：**构建系统里"传错 key"往往不会报错，而是静默降级**，
症状（只展开一帧）和原因（flag 没传进去）相隔很远，必须用 `--verbose` 落到
真实 rustc 命令行才能确认。

## 5. 实现与 PR 拆分思路

按小粒度拆分为 8 个 PR，全部已合并入上游 `rcore-os/tgoskits`：

| PR | 标题 | 核心内容 | 合并时间 |
| --- | --- | --- | --- |
| [#619](https://github.com/rcore-os/tgoskits/pull/619) | axbacktrace: raw backtrace report + ArceOS backtrace test | `axbacktrace` crate 重构：不依赖 DWARF 也能输出 raw `ip/fp` block；新增 ArceOS backtrace QEMU 测试 | 2026-05-15 |
| [#635](https://github.com/rcore-os/tgoskits/pull/635) | axbuild: improve backtrace symbolize robustness | host-side symbolizer 鲁棒性提升：处理前缀噪声、缺 END、重复 BEGIN；防止 success regex 假绿 | 2026-05-18 |
| [#646](https://github.com/rcore-os/tgoskits/pull/646) | arceos: add raw backtrace qemu test cases | 增加 `backtrace-raw-basic`、`backtrace-raw-normal`、`backtrace-raw-badfp` 三个边界 QEMU 用例 | 2026-05-18 |
| [#748](https://github.com/rcore-os/tgoskits/pull/748) | refactor(axbacktrace): replace BacktraceReport with Backtrace::kind() | 统一输出格式：把 `BacktraceReport` 重构为链式 `Backtrace::kind(kind)`，格式收归 `BACKTRACE_BEGIN/BT/BACKTRACE_END` | 2026-05-19 |
| [#749](https://github.com/rcore-os/tgoskits/pull/749) | feat(axbuild): auto symbolize backtrace after ArceOS rust QEMU tests | `cargo xtask arceos test qemu` 在 QEMU 输出 raw block 闭合后自动调用 host symbolizer | 2026-05-19 |
| [#793](https://github.com/rcore-os/tgoskits/pull/793) | axbuild: stream host backtrace symbolize when raw block ends | 流式打印：raw block 每闭合一次立即输出符号化结果，不等全部测试跑完 | 2026-05-20 |
| [#1023](https://github.com/rcore-os/tgoskits/pull/1023) | fix(axbuild): use target spec stem as rustflags config key | **关键 bug 修复**：修复 #839 引入的 `--config target.<key>.rustflags` 静默失效，恢复四架构 FP 链 unwind（详见 §4.1） | 2026-05-29 |
| [#1020](https://github.com/rcore-os/tgoskits/pull/1020) | test(starry-kernel): add memtrack alloc backtrace e2e | StarryOS `/dev/memtrack` 接入 raw block / host symbolize 框架；新增 `sample`/`sample_hard`/`symbolize` 调试命令；E2E 覆盖 host symbolize 链路（详见 §9） | 2026-05-29 |

这种拆分便于 review：每个 PR 都能独立说明目标、风险和验证命令。#1023 和 #1020 是本次实验的后期补充——前者修复了一个在 #1020 开发过程中才被 `sample_hard` 暴露出的构建回归，两者有依赖关系（#1023 应先合入）。

## 6. 测试策略

### 6.1 单元测试

代表性命令：

```bash
cargo test -p axbacktrace --features alloc
cargo test -p axbuild backtrace::
cargo clippy -p axbuild --all-targets -- -D warnings
cargo clippy -p tg-xtask --all-targets -- -D warnings
```

### 6.2 QEMU E2E

代表性命令：

```bash
cargo xtask arceos test qemu --arch x86_64 --test-group rust --test-case backtrace-raw-basic
cargo xtask arceos test qemu --arch x86_64 --test-group rust --test-case backtrace-raw-normal
cargo xtask arceos test qemu --arch x86_64 --test-group rust --test-case backtrace-raw-badfp
```

通过标准不能只看 `BACKTRACE_BEGIN`。应至少确认：

- 出现完整 raw block；
- 有 `BT 0`；
- normal case 至少有更深帧；
- `ip` 非零；
- 最终 test pass；
- 启用自动 symbolize 时终端出现 `=== host backtrace symbolize ===`。

### 6.3 panic 路径

panic 路径不直接作为“会通过”的 CI case，因为默认 fail regex 会匹配 `panic` 字样。因此 panic path 更适合手动 QEMU 验证：

```bash
cargo xtask arceos qemu --arch x86_64 \
  --package arceos-backtrace-raw-basic \
  --config test-suit/arceos/rust/backtrace-raw-basic/build-x86_64-unknown-none-panic.toml \
  --qemu-config test-suit/arceos/rust/backtrace-raw-basic/qemu-x86_64.toml
```

## 7. 学到的 OS 知识

1. **backtrace 不是一个动作，而是 unwind + symbolize。**
2. **FP 链是当前 unwind 策略的关键。** 没有 FP，只靠 IP 不能可靠找到上一帧。
3. **panic/trap 路径要尽量简单。** 复杂解析应该移到 host。
4. **日志格式是接口。** 一旦 host 工具和 CI 依赖它，就必须稳定。
5. **测试 regex 要防假绿。** 只匹配 begin 不代表 backtrace 成功。
