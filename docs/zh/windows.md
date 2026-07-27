---
title: Windows
description: 在 Windows 上安装并运行 Open Interpreter。
---

从 PowerShell 安装：

```powershell
irm https://www.openinterpreter.com/install.ps1 | iex
```

重启终端，然后验证：

```powershell
interpreter --version
```

## WSL

WSL 在项目已经使用 Linux 工具链时是一个不错的选择。在 WSL 中使用 macOS/Linux 命令进行安装：

```bash
curl -fsSL https://www.openinterpreter.com/install | sh
```

## 路径和 Shell

使用项目所期望的相同 Shell 和路径风格。原生 Windows 项目应使用 Windows 路径和 PowerShell 约定。WSL 项目应使用 Linux 路径和工具。

## 沙箱说明

原生 Windows 沙箱的强制细节与 macOS 和 Linux 不同。如果需要 Linux 风格的沙箱行为，请使用 WSL。对于受信任的本地仓库，先使用默认权限，仅在任务需要时才放宽权限。
