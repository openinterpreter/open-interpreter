---
title: 权限
description: 为文件系统和网络访问定义可重用的最小权限配置文件。
---

权限配置文件是相较于旧的 `sandbox_mode` 和 `sandbox_workspace_write` 设置更细粒度的新版方案。一次只能使用一种系统：如果活动配置中设置了 `sandbox_mode` 或者你使用了 `--sandbox` 参数，旧的沙箱设置将拥有更高优先级。

## 内置配置文件

| 配置文件 | 目的 |
| ------- | ------- |
| `:read-only` | 本地命令只读执行。 |
| `:workspace` | 工作区根目录内的读写访问。 |
| `:danger-full-access` | 没有本地沙箱限制。 |

选择一个配置文件：

```toml
default_permissions = ":workspace"
```

## 自定义配置文件

```toml
default_permissions = "project-edit"

[permissions.project-edit.workspace_roots]
"~/code/app" = true
"~/code/shared-lib" = true

[permissions.project-edit.filesystem]
":minimal" = "read"

[permissions.project-edit.filesystem.":workspace_roots"]
"." = "write"
".devcontainer" = "read"
"**/*.env" = "deny"

[permissions.project-edit.network]
enabled = true

[permissions.project-edit.network.domains]
"api.openai.com" = "allow"
"**.github.com" = "allow"
"tracking.example.com" = "deny"
```

此配置文件为常见的运行时路径提供读取权限，允许在工作区根目录写入，保持 `.devcontainer/` 为只读，拒绝 `.env` 文件，并将网络流量限制在选定的域名。

## 文件系统规则

| 访问 | 含义 |
| ------ | ------- |
| `read` | 命令可以读取和列出文件。 |
| `write` | 命令可以读取、创建、更新、重命名和删除文件。 |
| `deny` | 命令不能读取或写入匹配的路径。拒绝优先于更宽松的授权。 |

支持的根路径包括：

| 根路径 | 含义 |
| ---- | ------- |
| `:minimal` | 运行常见工具所需的运行时路径。 |
| `:workspace_roots` | 当前工作区根目录加上配置文件定义的根目录。 |
| `:tmpdir` | 当前临时目录。 |
| `:root` | 文件系统根目录。请谨慎使用。 |
| `/absolute/path` | 具体的绝对路径。 |
| `~/path` | 用户主目录下的路径。 |

尽可能使用精确路径。像 `**/*.env` 这样的拒绝通配符对包含机密的文件很有用；在某些平台上可能需要设置 `glob_scan_max_depth` 值以限制启动时的扫描深度。

## 网络规则

网络访问在权限配置文件中默认是禁用的。请显式开启：

```toml
[permissions.project-edit.network]
enabled = true
```

然后允许或拒绝域名：

```toml
[permissions.project-edit.network.domains]
"example.com" = "allow"
"*.example.com" = "allow"
"**.example.com" = "allow"
"ads.example.com" = "deny"
```

模式说明：

| 模式 | 含义 |
| ------- | ------- |
| `example.com` | 完全匹配主机。 |
| `*.example.com` | 仅子域名。 |
| `**.example.com` | 顶级域名及子域名。 |
| `*` | 广泛的公共允许。请有意使用。 |

本地和私有网络目的地有单独的保护。需要时可允许字面目标如 `localhost` 或 `127.0.0.1`。

## Unix 套接字

Unix 套接字允许规则是针对 Docker 等工具的本地逃生通道：

```toml
[permissions.project-edit.network.unix_sockets]
"/var/run/docker.sock" = "allow"
```

仅在工作流真正需要该本地服务时使用。

## 作用范围

权限配置文件管理本地沙箱化的命令执行。应用连接器、MCP 服务器、浏览器/计算机使用界面、已批准的提权以及远程服务都有各自的控制机制。
