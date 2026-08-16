---
title: 技能
description: 打包可重用的工作流、参考资料、脚本和资源。
---

技能是一个包含 `SKILL.md` 文件以及可选支持文件的文件夹。Open Interpreter 首先读取技能元数据，只有当请求匹配时才加载完整技能。

## 文件夹结构

```text
cut-release/
├── SKILL.md
├── scripts/
├── references/
└── assets/
```

`SKILL.md` 为必需文件。其他目录为可选。

## 最小化技能示例

```markdown
---
name: cut-release
description: 通过测试、更新变更日志并打标签来准备发布。
---

当被要求进行发布时：

1. 运行测试套件。
2. 更新变更日志。
3. 根据语义化版本号（semver）提升版本号。
4. 准备提交和标签，但在发布前询问确认。
```

`description` 决定技能何时被选中，因此请具体描述。

## 位置

| 路径 | 范围 |
| ---- | ----- |
| `.agents/skills/` | 仓库或目录本地的技能 |
| `~/.agents/skills/` | 个人技能 |
| 内置技能 | 内置工作流 |

当名称冲突时，本地技能的优先级高于个人和内置技能。

新的用户技能应放在 `.agents/skills` 目录中。这是一个工具无关的共享目录，因此其他兼容的智能体也可以直接使用同一个技能，无需为 Open Interpreter 导入或复制。

Open Interpreter 还会读取 `~/.openinterpreter/skills/`，以保持向后兼容。新的用户自定义技能不应放在这个产品专用目录中。内置技能仍可能缓存在 Open Interpreter 主目录下，因为它们属于产品资源，而不是可移植的用户数据。

### 内置技能更新

Open Interpreter 管理 `$INTERPRETER_HOME/skills/.system/`（通常为 `~/.openinterpreter/skills/.system/`）中的内置技能缓存。运行时会为完整的内置技能包生成指纹；新安装的 Open Interpreter 版本如果包含已更改或已移除的技能，就会刷新该目录。更新后的运行时在启动交互式、`exec` 或 app-server 会话时都会执行此检查。

请将 `.system/` 视为只读目录：Open Interpreter 更新可能会替换其中的改动。用户技能、宿主应用技能以及 `skills/` 下的其他同级目录不属于 Open Interpreter 的托管命名空间，因此不会被这次刷新修改。

## 技能应包含的内容

- 发布检查清单
- 内部报告生成
- 仓库特定的迁移工作流
- 设计或审查标准
- 需要固定顺序的命令

保持 `SKILL.md` 简洁。将长篇参考资料放在 `references/`，可运行的辅助脚本放在 `scripts/`，模板放在 `assets/`。

## 工具与批准行为

技能脚本会通过常规的沙箱和批准控制运行。技能应描述脚本的功能以及何时适合运行，但不应依赖绕过权限。

## 浏览技能

在 TUI 中：

```text
/skills
```
