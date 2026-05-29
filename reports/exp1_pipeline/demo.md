# Exp1 Demo: polyglot-agent-harness

> 目标：展示本工作区的 agent workflow 入口、技能目录、Docker preflight 和本地报告位置。

## 环境准备

本 demo 的 Docker 部分只检查宿主机 Docker daemon，不需要进入项目容器。

## Demo 1: 查看统一规则入口

### 命令

```bash
sed -n '1,120p' polyglot-agent-harness/AGENTS.md
```

### 预期现象

输出包含：

```text
# Agent instructions (polyglot-agent-harness)
## Global rules
## Mandatory skill triggers (must)
## Pipeline (canonical)
```

## Demo 2: 查看技能目录

### 命令

```bash
find polyglot-agent-harness/skills -maxdepth 2 -name SKILL.md -print | sort
```

### 预期现象

输出包含：

```text
polyglot-agent-harness/skills/experiment-guard/SKILL.md
polyglot-agent-harness/skills/pr-workflow/SKILL.md
polyglot-agent-harness/skills/report-generator/SKILL.md
polyglot-agent-harness/skills/test-authoring/SKILL.md
```

## Demo 3: Docker preflight

### 命令

```bash
python3 polyglot-agent-harness/scripts/docker-check.py
```

### 预期现象

Docker 可用时返回成功状态，并显示 Docker daemon 可访问。

Docker 不可用时返回非零状态，并提示需要启动 Docker 或修复权限。

## Demo 4: 查看本地报告和 PR 草稿位置

### 命令

```bash
find . -maxdepth 2 -type d \( -name 'pr-drafts' -o -name 'reports-local' -o -name 'biglab b 汇报' \) -print
```

### 预期现象

输出包含本地材料目录，例如：

```text
./biglab b 汇报
```

## Demo 5: 运行 skill 触发演示脚本

### 命令

```bash
bash 'biglab b 汇报/exp1_pipeline/demo/run_harness_skill_demo.sh'
```

### 预期现象

输出包含：

```text
== polyglot-agent-harness live demo ==
[1/4] Harness entry
[2/4] Skill inventory
[3/4] Trigger simulation
EXPECTED FIRST LINE: Applying experiment-guard
EXPECTED FIRST LINE: Applying test-authoring
EXPECTED FIRST LINE: Applying report-generator
EXPECTED FIRST LINE: Applying pr-workflow
EXPECTED FIRST LINE: Applying auto-skill-maintainer
```

## Demo 6: 现场 prompt 触发 `test-authoring`

### 命令

把下面这段发给当前 agent：

```text
请准备一个 StarryOS QEMU 回归测试的方案。
请尝试发现 Starry OS 中的 一个 bug 并提 PR。
```

### 预期现象

agent 回复开头包含：

```text
Applying test-authoring
```

随后输出测试方案，内容包含测试落点、`build-*.toml` / `qemu-*.toml`、success/fail regex 和本地验证命令。
