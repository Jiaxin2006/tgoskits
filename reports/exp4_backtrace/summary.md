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

## 8. 展示时可讲的重点

- 用一张图讲清 target/host：

```text
QEMU guest target:
  BACKTRACE_BEGIN / BT ip/fp / BACKTRACE_END

Host:
  cargo xtask backtrace symbolize
  ELF + addr2line -> function/file/line
```

- 用一张图讲清 FP/IP：

```text
fp -> previous fp
   -> saved return address ip
```

- 重点解释为什么 host symbolize 更 robust。

## 9. StarryOS memtrack backtrace E2E（`feat/starry-memtrack-backtrace`）

> 说明：alloc tracking 的真实 caller stack 助教确认**可以不作为本轮硬性目标**。
> 这里是在主线之外额外实现的：既验证「raw block → host symbolize」框架对真实场景
> （而非合成测试）的通用性，又恰好通过 `sample_hard` 探针暴露并帮助定位了 §4.1 的
> #839 帧指针回归。帧指针修复在分支 `fix/memtrack-raw-backtrace`（`a77b21966`），
> 本 memtrack 分支需叠加该修复才能看到 `sample_hard` 完整展开。

在 arceos 侧 raw backtrace 框架稳定后，将同一套 raw block / host symbolize 链路接入 StarryOS `/dev/memtrack`，为 `ax-alloc/tracking` 记录的 allocation backtrace 补上可复现的 E2E 测试。

**关键设计变化**：

| 修改点 | 内容 |
| --- | --- |
| `starry-kernel/memtrack` Cargo.toml | 改为 `ax-feat/backtrace + ax-alloc/tracking`，移除 `gimli` 强依赖 |
| `/dev/memtrack` `sample\n` 命令 | 简单版：直接 `Vec::with_capacity(4096)`，走真实 allocator + `Backtrace::capture()` |
| `/dev/memtrack` `sample_hard\n` 命令 | 深调用链版：`mid -> leaf -> Vec::with_capacity(8192)`，用于手动观察真实 alloc unwind |
| `/dev/memtrack` `symbolize\n` 命令 | 确定性 probe：用 `capture_trap(0, ip, 0)` 输出单帧 raw block，供 host symbolize 断言 |
| `axbuild` Starry test runner | `BACKTRACE=y` 时自动捕获 raw block 并调用 host symbolizer |
| 新增 `host_symbolize_success_regex` | 允许 Starry E2E 对 host symbolizer 输出断言函数名 |
| 新测试用例 `memtrack-backtrace` | x86_64 QEMU，`BACKTRACE=y`，验证 guest 命令链路与 host symbolize probe |

**E2E 命令**：

```bash
cargo xtask starry test qemu --arch x86_64 --test-group normal --test-case memtrack-backtrace
```

**E2E guest 实际执行顺序**：

```bash
start -> sample -> sample_hard -> symbolize -> end
```

**E2E 真正断言的内容**：

| 层级 | 断言 | 不断言 |
| --- | --- | --- |
| guest | `sample` / `sample_hard` 成功提示行；至少一个 `kind=alloc` raw block；脚本完成标记 | alloc unwind 深度；`Memory usage:` 汇总里的函数名 |
| host | `symbolize` probe 被解析为 `starry_memtrack_symbolize_probe` | 真实 allocation frame 是否可 symbolize |
| 全局 | 无 panic | `sample_hard` 栈质量 |

**E2E 关键输出**：

```text
Memory allocation sample recorded
Hard memory allocation sample recorded
BACKTRACE_BEGIN kind=alloc arch=x86_64 alloc=true dwarf=false
BT 0 ip=0x... fp=0x...
BACKTRACE_END
STARRY_MEMTRACK_BACKTRACE_OK
=== host backtrace symbolize ===
BT 0 ip=0xffff8000003e5f71 fp=0x0 starry_memtrack_symbolize_probe
ok: memtrack-backtrace
```

**关于 fp=0x0 和只有一个 host symbolize 帧**：

- `symbolize\n` 使用 `capture_trap(0, ip, 0)`，不做 FP 链展开，因此 `fp=0x0`、单帧是预期行为（这是 probe 的设计，不代表真实 alloc 栈也是单帧）。
- 真实 alloc tracking 在 allocator 内调用 `Backtrace::capture()` 做 FP unwind；在帧指针修复后它能稳定展开（见上「根因复盘」）。
- E2E 仍把 `symbolize` probe 作为硬断言、把 `sample_hard` 真实栈作为观察项：这样即便后续在早期分配 / IRQ 上下文 / 被内联的分配点等固有难点上偶发不完整，CI 也不会 flaky。

**根因复盘：sample_hard 一度展不开调用者函数名 → 已定位并修复**：

`sample_hard` 的目标是制造更深的调用路径：

```text
record_hard_sample_allocation
  -> starry_memtrack_sample_hard_mid
    -> starry_memtrack_sample_hard_leaf
      -> Vec::with_capacity / resize
        -> GlobalAlloc::alloc
          -> axbacktrace::Backtrace::capture()
```

早期观测到的 raw alloc block 往往是空块或只有 `BT 0 ip=0x0 fp=0x0`，一度被当成
「allocator 捕获点 FP unwind 固有不稳定」的已知限制。

后续排查推翻了这个结论：**真正的主因是 §4.1 的 #839 帧指针注入回归**，
而不是 allocator 特有问题。`-C force-frame-pointers=yes` 被静默丢弃后，
整个内核都没有可靠的 FP 链，`Backtrace::capture()` 自然只能记录空帧——
这同时解释了为什么连普通 `backtrace-raw-normal` 也只展开一帧。

修复帧指针 config key（`fix/memtrack-raw-backtrace` 的 `a77b21966`）后重跑，
`sample_hard` 能稳定展开真实调用链并被 host symbolizer 解析：

```text
=== host backtrace symbolize ===
BACKTRACE_BLOCK 0 kind=alloc arch=x86_64
BT 0  ... starry_memtrack_sample_hard_leaf
BT 1  ... starry_memtrack_sample_hard_mid
BT 2  ... <probe handler>
...   ... ax_fs_ng / ax_task 调度链
BT N  ip=0x0 fp=0x0
```

因此修正后的结论是：

- allocator/tracking 已接入 raw `Backtrace` 记录；
- memtrack 已能输出 `kind=alloc` raw block；
- host symbolizer 已能解析确定性 probe；
- **真实 allocator capture 的调用者栈在帧指针修复后可以稳定展开为函数名**
  （`BACKTRACE=y` 只给函数名、无文件行号；要行号需 `DWARF=y`）。
  早先「未解决」的判断属于被 #839 回归误导，现已澄清；
- `sample_hard` 因此从「暴露 bug 的诊断工具」转为「可正常展开的真实 alloc 示例」，
  但出于稳健性仍保留为观察项而非 CI 硬断言（见下）。

**手动观察真实 alloc unwind**：

```bash
printf 'start\n' > /dev/memtrack
printf 'sample_hard\n' > /dev/memtrack
printf 'end\n' > /dev/memtrack
```

重点看 `Memory usage:` 段落与 host symbolize 段落；帧指针修复后可稳定看到 `starry_memtrack_sample_hard_leaf` / `starry_memtrack_sample_hard_mid`。

**测试结果（本地 Docker + 上游 CI）**：

| 验证层 | 命令 / 结果 |
| --- | --- |
| axbuild 单测 | `cargo test -p axbuild backtrace::` — 31 passed |
| axbuild starry 单测 | `cargo test -p axbuild starry::test::` — 29 passed |
| ArceOS E2E x86_64 | `cargo xtask arceos test qemu --arch x86_64 --test-group rust --test-case backtrace-raw-normal` — 通过，8 帧展开 + host symbolize |
| Starry memtrack E2E | `cargo xtask starry test qemu --arch x86_64 --test-group normal --test-case memtrack-backtrace` — 通过 |
| Starry clippy | `FEATURES=starry-kernel/memtrack cargo xtask clippy --package starry-kernel` — 通过 |
| 上游 CI | PR #619/#635/#646/#748/#749/#793/#1023/#1020 全部通过 CI 并合并入 `rcore-os/tgoskits` upstream `dev` |

**已知限制**：

- 当前 E2E 只覆盖 x86_64 QEMU。
- macOS 宿主机需确保 `debugfs` 在 PATH 中，否则 Starry rootfs 注入阶段会失败。
- 真实 allocation frame 能否 symbolize 取决于 FP 保留；在帧指针修复（`a77b21966`）后可稳定展开为函数名。本 PR 仍以 `symbolize` probe 做硬断言、`sample_hard` 做观察。
- `sample_hard` 完整展开依赖帧指针修复；`feat/starry-memtrack-backtrace` 单独构建（不含 `a77b21966`）仍受 #839 回归影响，会退化为空块/单帧。
- 即使帧指针正常，alloc tracking 仍有固有难点：早期分配（`axbacktrace::init()` 设 `FP_RANGE` 之前）、IRQ 上下文跨 trap、被内联/尾调用的分配点，可能得到不完整的真实 alloc 栈。

## 10. PR 策略建议：是否现在提交上游

建议提交一个**阶段性、范围收窄的 PR**，不要把它描述成“alloc tracking 已完整解决调用者函数名解析”。

适合当前 PR 的边界：

- `starry-kernel/memtrack` 从 target-side DWARF/gimli 分类中解耦，改为 `BACKTRACE=y + ax-alloc/tracking`；
- `/dev/memtrack` 将 allocation tracking 记录输出为 `kind=alloc` raw block；
- Starry QEMU E2E 覆盖 guest raw block 输出；
- Starry QEMU runner 覆盖 host-side symbolize，且通过 deterministic `symbolize` probe 验证函数名解析。

边界与依赖：

- 帧指针修复（`a77b21966`）应作为前置 / 一起合入；memtrack 分支单独构建仍受 #839 回归影响；
- CI 硬断言用确定性 `symbolize` probe，`sample_hard` 真实栈作为观察项（避免 IRQ / 早期分配 / 内联等固有边角导致 flaky）；
- 不在 allocator/tracking 层引入复杂分类或展示语义。

理由：

- 当前改动有清晰价值：把 Starry memtrack 接入通用 raw block / host symbolize 框架，去掉 target-side DWARF 强耦合，并顺带定位修复了 #839 帧指针回归；
- 帧指针修复后真实 alloc 调用链可稳定展开为函数名，但把它写成"已彻底解决一切 alloc 栈"仍不严谨——固有难点（早期/IRQ/内联）依旧存在，因此用 probe 做硬断言、真实栈做观察是更稳的表述；
- 上游 review 更容易接受一个"可复现、边界清晰、承认依赖与限制"的 PR。

## 11. 可回答的追问

**Q: target/host 和 S/U 态有什么区别？**  
A: target/host 是运行环境划分。target 是 QEMU/真机里的 OS，host 是开发机或 CI。S/U 态是同一个 target 内部的特权级。

**Q: raw block 里有没有函数名？**  
A: 没有。raw block 只有 `ip/fp` 地址。函数名和行号是 host 用 ELF/DWARF 后处理出来的。

**Q: 为什么只输出 IP 不够？**  
A: IP 描述当前帧位置，但无法找到上一帧；FP 链提供了向上遍历栈的结构。

**Q: 为什么 panic 路径不做 CI pass case？**  
A: 测试 harness 默认把 `panic` 字样当 fail pattern。真实 panic 输出是预期现象，但会被 harness 判失败，因此适合手动验证或单独设计规则。

**Q: memtrack E2E 里 fp 为什么是 0x0？**  
A: `symbolize\n` 探针是人工合成的单帧 block，不做 FP 链展开，fp=0x0 是预期行为。真实 allocation frame 的 FP 保留取决于构建配置，属于已知限制。

**Q: `sample_hard` 全挂了 E2E 还能过吗？**  
A: 分两种情况。若只是真实 alloc backtrace 质量差（无多层 unwind、无法 symbolize），但命令本身打印了 `Hard memory allocation sample recorded` 且 `symbolize` probe 通过，则 E2E 仍会通过；若 `sample_hard` 命令本身失败或 panic，则 E2E 会失败。这样设计是为了不让固有边角情况导致 CI flaky。

**Q: sample_hard 一开始看不到函数名，到底是什么原因？**  
A: 不是 allocator 固有问题，而是 #839 把帧指针 flag 的 cargo config key 写成了 spec 文件路径、被 cargo 静默丢弃（见 §4.1）。修复 config key 后真实 alloc 调用链能稳定展开为 `starry_memtrack_sample_hard_leaf` / `..._mid` 等函数名。`sample_hard` 这个探针正是定位该回归的关键样本。

**Q: 助教说 alloc tracking 可以不做，为什么还实现了？**  
A: 它是验证「raw block → host symbolize」框架对真实场景通用性的最佳样本，且实现过程中通过 `sample_hard` 暴露并帮助定位了 #839 帧指针回归。属于本轮硬性要求之外的额外实现 + 有价值的回归诊断。
