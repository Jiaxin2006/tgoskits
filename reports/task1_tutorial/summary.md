# Task1 Report: tg-arceos-tutorial

> 任务仓库：<https://github.com/Jiaxin2006/tg-arceos-tutorial>  
> 本地材料：`tg-arceos-tutorial/docs/report_T1.md`、`tg-arceos-tutorial/README.md`  
> 展示定位：通过 ArceOS 教学实验建立 OS 分层意识，为后续 StarryOS 兼容性修复打基础。

## 1. 实验目标

`tg-arceos-tutorial` 是 ArceOS 教学 crate 的集合，包含 `app-*` 示例和 `exercise-*` 练习。Task1 的目标不是完成一个单点功能，而是熟悉 TGOSKits/ArceOS 的构建、运行、测试方式，并通过几个小实验理解：

- `no_std + alloc` 环境中，哪些能力需要自己补齐；
- 文件系统操作如何穿过 `axstd -> axfs -> axfs_ramfs`；
- 分配器如何管理字节分配与页分配；
- 用户态 syscall 如何进入内核，并在用户地址空间中建立映射；
- 为什么调试时必须沿调用链定位，而不是只改第一个搜到的函数。

## 2. 实验内容总览

| 练习 | 主题 | 核心 OS 概念 |
| --- | --- | --- |
| `exercise-printcolor` | 彩色串口输出 | bare-metal 输出、ANSI 控制序列、QEMU serial |
| `exercise-hashmap` | `HashMap` 支持 | `no_std`、`alloc`、Cargo feature、path dependency |
| `exercise-altalloc` | bump allocator | 内存分配、对齐、byte/page allocator |
| `exercise-ramfs-rename` | ramfs `rename` | VFS 分发、目录项、锁与死锁 |
| `exercise-sysmap` | `mmap` syscall | 用户地址空间、页映射、文件映射 |

## 3. 实验分析

### 3.1 `exercise-ramfs-rename`: rename 是目录项操作

这个实验表面上是实现 `std::fs::rename`，实际调用链是：

```text
用户程序 std::fs::rename
  -> axstd
  -> axfs root API
  -> RootDirectory 分发
  -> axfs_ramfs::DirNode::rename
```

 rename 不用理解成“移动文件内容”，实际更准确的理解是：rename 改的是目录树里的 name -> node 映射。inode/节点本身不应该被复制，文件内容也不应该被重写。

关键 bug 是：只在 `axfs_ramfs` 的 `DirNode` 实现 `rename` 不够，`RootDirectory` 仍然可能使用默认实现返回 `Unsupported`。因此修复时必须让根目录层把 rename 转发给实际挂载的文件系统。

另一个关键点是同目录 rename 的锁问题。`spin::RwLock` 不支持重入，如果同一个目录上先 remove 再 insert 时重复拿写锁，可能死锁。因此实现中需要识别源目录和目标目录是否相同，并在同一个写锁作用域内完成目录项移动。

### 3.2 `exercise-sysmap`: `mmap` 是地址空间操作

`sys_mmap` 的最小流程是：

```text
用户态 mmap(fd, len, prot, flags, offset)
  -> syscall trap
  -> 内核校验 length/prot/flags/fd/offset
  -> 在 USER_ASPACE 中分配虚拟地址
  -> 建立页表映射
  -> 如果是文件映射，从 fd 读取内容
  -> 写入用户映射地址
  -> 返回用户虚拟地址
```

这个实验让我理解到，`mmap` 不是“返回一个指针”这么简单。真正重要的是：

- `length` 和 `offset` 要符合页映射约束；
- `PROT_READ/WRITE/EXEC` 要转换成内核页表权限；
- `MAP_PRIVATE/MAP_SHARED/MAP_ANONYMOUS/MAP_FIXED` 影响映射来源与地址选择；
- 文件内容要通过内核文件对象读出，再写进用户地址空间；
- 返回的虚拟地址必须在用户态可访问。

这些理解后来迁移到了 StarryOS 的 Linux ABI 修复中，例如 `mmap(fd=0)`、用户指针 copyin/copyout、poll/epoll 等问题。

### 3.3 `exercise-altalloc`: 分配器的取舍

`exercise-altalloc` 实现的是双端 bump allocator：

```text
[ byte used -> | free space | <- page used ]
start        b_pos        p_pos          end
```

- byte allocation 从低地址向高地址增长；
- page allocation 从高地址向低地址增长；
- byte deallocation 不回收单个对象，只在活跃 byte allocation 计数归零时整体重置；
- page deallocation 基本不做精细回收。

这个实验说明 allocator 设计必须围绕使用场景取舍。bump allocator 的优势是简单、快、状态少，适合 early boot；缺点是释放能力弱，不适合长期复杂负载。

## 4. 复现与排错经验

Task1 的测试不只是看源码是否写对，还要保证 ArceOS 依赖图可复现。一次典型问题是 `exercise-printcolor` 在 x86_64 下没有输出 `Hello, Arceos!`，日志最后实际是链接失败：

```text
rust-lld: error: duplicate symbol: __PERCPU_SELF_PTR
```

这个现象容易误判成 `println!` 或 ANSI 输出写错，但根因是构建根本没有进入运行阶段。排查时先看 `cargo tree`，可以发现同一个实验里混入了两个 `percpu` 版本：

```text
percpu@0.2.3-preview.1
percpu@0.4.0
```

`percpu@0.4.0` 是由漂移到较新版本的 `axcpu v0.3.1` 带进来的，而当前 `axstd/axhal/axplat` 预览版依赖栈仍应使用旧的 `percpu` 分支。两个版本都会定义 `__PERCPU_SELF_PTR`，所以 x86_64 链接阶段报 duplicate symbol。

解决方式不是改实验业务代码，而是恢复 lockfile 的一致性：将 `exercise-printcolor/Cargo.lock` 和同样受影响的 `exercise-altalloc/Cargo.lock` 中的 `axcpu` 固定回 `0.3.0-preview.6`，确保只保留 `percpu@0.2.3-preview.1`。修复后再用 Docker 容器跑五个实验的四架构脚本验证。

推荐复现命令：

```bash
cd tg-arceos-tutorial
docker run --rm \
  -v "$PWD":/workspace \
  -w /workspace \
  ghcr.io/rcore-os/tgoskits-container:latest \
  bash -lc 'for d in exercise-printcolor exercise-hashmap exercise-altalloc exercise-ramfs-rename exercise-sysmap; do echo "===== $d ====="; (cd "$d" && bash scripts/test.sh) || exit 1; done'
```

完整通过时，每个实验都会显示：

```text
Passed:  4 / 4
Failed:  0 / 4
Skipped: 0 / 4
```

如果宿主机或容器缺少某些 QEMU，脚本可能显示 skipped；展示和最终验收时建议使用 `ghcr.io/rcore-os/tgoskits-container:latest`，保证工具链和 QEMU 环境一致。

## 5. 学到的 OS 知识

1. **调用链比 API 名称重要。** `rename`、`mmap`、`HashMap` 都不是只改当前 crate 就结束，必须知道请求如何穿过上层抽象。
2. **`no_std` 环境没有默认舒适区。** 桌面 Rust 默认可用的集合、随机数、文件接口，在 ArceOS 中都需要显式依赖和 feature。
3. **VFS 操作要同时看分发层和具体 FS。** 只改 `axfs_ramfs` 不能保证 `axfs` 根目录会调用到它。
4. **内存管理 bug 常在边界。** 对齐、上下界相撞、释放计数、页/字节两类分配之间的空间竞争都必须检查。
5. **用户态 syscall 不是普通函数调用。** 用户参数不可信，返回值、地址空间和权限都需要按 OS 语义处理。

