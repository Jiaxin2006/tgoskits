# Exp1 Report: polyglot-agent-harness

> 本地入口：`polyglot-agent-harness/AGENTS.md`  
> 技能目录：`polyglot-agent-harness/skills/*/SKILL.md`  
> 目标定位：把 AI 协作变成可复现、可验证、可审查的 OS 工程流程。

## 1. 实验目标

实验一的目标不是让 AI 偶尔生成一段补丁，而是建立一个跨平台（因为我在每个平台都没有足够多的 token）的 agent workflow harness。它把不同 AI 工具中的协作规则、技能、报告和 PR 准备流程统一起来，使后续 StarryOS/TGOSKits 修改可以高效执行。

核心问题是：OS 实验中的 bug 往往跨越源码、构建、QEMU、rootfs、测试脚本和 CI。如果只靠一次 prompt，很容易得到不可复现的修改；如果没有固定报告和验证流程，也很难在答辩或 review 中解释清楚。

## 2. 改进 harness 的聊天记录

会话日志：

| 日期 / 会话线索 | 当时的问题 | 对 harness 的影响 |
| --- | --- | --- |
| 2026-05-26 `rollout-2026-05-26T10-14-24...` | “怎么让你自动知道使用这个文件夹下的 skills？跑对应指令的时候，应该在 docker 环境里面。” | 在工作区根目录新增 `AGENTS.md`，把 `polyglot-agent-harness/AGENTS.md` 作为统一入口，并把 Docker policy 写成默认规则。 |
| 2026-05-26 `rollout-2026-05-26T10-33-48...` | backtrace / memtrack 工作中需要同步 upstream、跑 Docker、生成报告、准备 PR 草稿。 | 实际触发并使用了 `experiment-guard`、`test-authoring`、`report-generator`、`pr-workflow`，证明 skill 不是静态文档，而是被真实开发流程调用。 |
| 2026-05-26 同一会话后续 | “本地不想每次丢 rustup 状态的话，别用 docker run --rm，用 named container 或挂载 cache。” | 触发 `auto-skill-maintainer`，把 reusable container / cache volume 写进 harness，最后形成 commit `54bba1c harness: document reusable docker verification`。 |

本地会话日志、`polyglot-agent-harness` git history 和现有 `SKILL.md` 能还原出一条清楚的演进链：先让 agent 自动读取本地 skills，再把 Docker、报告、PR、测试、完成度复核这些重复流程逐步固化。

## 3. Harness 结构

`polyglot-agent-harness` 的关键组成包括：

| 组件 | 作用 |
| --- | --- |
| `AGENTS.md` | 统一规则入口，说明全局流程与强制技能触发条件 |
| `skills/*/SKILL.md` | 可复用工作流，例如 PR、报告、测试、实验保护 |
| `agents/*.md` | 子 agent 风格的 review / triage 角色提示 |
| `commands/*.md` | 可复用命令流程，例如默认验证 pass |
| `scripts/docker-check.py` | Docker preflight，保证构建/测试环境接近 CI |
| `docs/PORTABILITY.md` | 描述 Claude/Cursor/Codex/Trae 的接入方式 |
| `pr-drafts/` | 保存 PR 草稿，避免把临时报告混入上游 PR |

因此它不只是“多个 skill 的简单集合”。更准确的理解是：

- `AGENTS.md` 负责路由：什么情况下必须触发哪个 skill；
- `skills/*/SKILL.md` 负责过程知识：每类任务怎么做、做到什么程度；
- `scripts/` 负责可执行检查：例如 Docker daemon 是否可用；
- `reports-local/` 与 `pr-drafts/` 负责证据沉淀：把一次实验变成可复盘材料；
- `.cursor/rules`、`docs/PORTABILITY.md` 等负责跨工具复用：同一套规则能迁移到 Claude、Cursor、Trae、Codex。

强制触发的代表技能：

- `experiment-guard`：开始 risky work、同步 upstream、merge/rebase 前使用；
- `test-authoring`：写或修改测试前使用；
- `report-generator`：出现失败、修复 bug、准备 PR 时生成结构化报告；
- `pr-workflow`：准备 PR 时检查范围、验证、描述和证据；
- `completion-examiner`：声明完成前做复核。

## 4. 每个 skill 蕴含的功能

| Skill | 它解决的问题 | 具体功能 |
| --- | --- | --- |
| `experiment-guard` | OS 实验里同步上游、merge/rebase、Docker 验证都容易弄脏状态。 | fetch upstream、确认 base branch、冲突立即停止、优先 Docker/CI-like 验证，并记录当前分支、commit、验证命令。 |
| `test-authoring` | 测试不是随便写一个程序，而是要能被 `cargo xtask ... test qemu` 稳定发现和判定。 | 选择 Rust/C/script 测试形态，规定 `test-suit` 落点，补齐 `build-*.toml` / `qemu-*.toml`，写 success/fail regex，并避免假绿。 |
| `report-generator` | 失败、bug 和 PR 如果只留口头描述，很难答辩或 review。 | 用固定模板记录 TL;DR、环境、复现步骤、现象、根因、修复、验证、风险和 PR 拆分。 |
| `pr-workflow` | 上游 PR 需要小范围、明确标题、可复制 test plan 和证据。 | 检查 diff 范围，要求 fmt/clippy/tests/E2E 顺序，生成 Conventional Commits 标题和中文 PR body，默认只写本地草稿，不自动开 PR。 |
| `completion-examiner` | “我觉得完成了”不等于可以交付。 | 从验收标准出发列疑点，把疑点变成测试或证据，最后再说明剩余风险。 |
| `auto-skill-maintainer` | 重复踩坑如果不沉淀，下次还会发生。 | 发现 workflow 缺口后更新或新增 skill，并在 harness 仓库中形成 focused commit。 |
| `harness-overview` | 同一套规则要被不同 agent 工具理解。 | 解释 Claude/Cursor/Trae/Codex 的安装和映射路径。 |

## 5. 在实验过程中如何改进这些 skill

从 `polyglot-agent-harness` 的 commit history 可以看到改进是逐步发生的：

| Commit | 改进内容 | 对应实验中的触发原因 |
| --- | --- | --- |
| `79b5f78 feat: add report-generator skill` | 增加结构化报告能力。 | backtrace、syscall、BusyBox 这类工作需要离线复现证据，不能只靠聊天记录。 |
| `5de8399 harness: add test-authoring and auto skill maintainer` | 增加测试写作和技能维护机制。 | OS 测试有目录、配置、regex 和 QEMU runner 约束，反复踩坑需要沉淀。 |
| `f0d57cd harness: enforce reporting and skill-maintenance pipeline` | 把报告与 skill 更新纳入默认流程。 | 发现新坑后不能只在当前对话里解决，要写回 harness。 |
| `c0aaa8f harness: add completion-examiner and CI disk-full playbook` | 增加完成度复核和 CI 失败 playbook。 | PR 不能只看本地 happy path，CI 资源失败也要分类记录。 |
| `b4fa35e harness: add mandatory skill triggers` | 明确什么情况下必须应用哪个 skill。 | 避免 agent “忘记”使用流程。 |
| `092762e` / `a90a751` / `0a3e5a0` | 增加 fmt、clippy、测试和 PR draft 规则。 | tgoskits PR 准备中，格式化、clippy 和 upstream-facing Test plan 都需要更清楚边界。 |
| `13ad8a2 harness: conventional PR titles and visible skill declaration` | 要求 PR 标题规范，并且应用 skill 时可见声明。 | 展示 demo 中可以直接看到 agent 回答开头的 `Applying <skill>`。 |
| `54bba1c harness: document reusable docker verification` | 记录 named container / cache volume，避免反复丢 rustup 状态。 | backtrace/memtrack 验证中发现 `docker run --rm` 会导致工具链反复下载。 |

这说明实验一不是单纯“配置 AI 工具”，而是把实际 OS 开发中的失败模式抽象成可复用流程。每次改进都来自一个真实问题：Docker 环境不稳定、测试可能假绿、PR 描述可能混入本地计划、完成状态可能缺证据。

## 6. 工作流设计

整体流程可以理解成：

```text
读取本地规则
  -> Docker/环境预检
  -> 小目标开发
  -> 运行验证
  -> 生成报告
  -> 准备 PR 草稿
  -> 人工 review / 上游提交
```

这套 workflow 的重点是把 AI 能力和确定性流程分开：

- AI 适合做代码阅读、调用链整理、初版实现、解释差异；
- 脚本和规则适合做环境检查、验证命令、报告落盘、PR 描述结构；
- 人负责最终判断语义是否正确、PR 是否应该提交。

## 7. 对后续实验的作用

后续 StarryOS syscall、BusyBox、backtrace 工作都受益于这个 harness：

1. **Docker policy** 让验证尽量在项目容器中完成，减少“我机器能跑”的不确定性。
2. **报告流程** 让每个 bug 有现象、根因、修复、测试和限制，而不是只留下最终代码。
3. **PR workflow** 强制小粒度提交和明确 test plan，方便 reviewer 复核。
4. **技能维护机制** 提醒我们把反复踩坑的流程沉淀下来，而不是靠记忆。

## 8. 学到的 OS 工程经验

OS 开发不是只写内核代码。真实工作还包括：

- 同步上游并避免错误合并；
- 选择和 CI 一致的构建环境；
- 为 QEMU/test-suit 写可长期运行的验证；
- 记录失败样本和限制；
- 把一次调试经验转成可复用 checklist。

因此，harness 的意义不是“让 AI 更自由”，而是相反：让 AI 在明确边界里工作。


