---
title: 安装
description: 在 macOS、Linux 或 Windows 上安装或更新 Open Interpreter CLI。
---

公共安装程序会下载适用于您平台的正确发行版，并安装 Open Interpreter 自更新逻辑使用的受管独立布局。

<Tabs>
  <Tab title="macOS / Linux">
    ```bash
    curl -fsSL https://www.openinterpreter.com/install | sh
    ```
  </Tab>
  <Tab title="Windows PowerShell">
    ```powershell
    irm https://www.openinterpreter.com/install.ps1 | iex
    ```
  </Tab>
</Tabs>

安装后请重新启动您的 shell，然后验证二进制文件：

```bash
interpreter --version
```

## 要求

| 项目 | 说明 |
| ---- | ----- |
| macOS | 当前发行版构建面向现代 macOS 版本。 |
| Linux | 使用近期的 64 位发行版。发行压缩包使用 musl 以实现广泛兼容。 |
| Windows | 使用 PowerShell 进行安装。也支持在 WSL 中进行类 Linux 工作流。 |
| Git | 推荐用于支持仓库感知的会话、差异和审阅。 |

## 更新

独立安装可以在正常交互式启动时检查更新。您也可以显式运行安装程序获取最新发行版：

```bash
interpreter update
```

在配置中将 `check_for_update_on_startup = false` 设置为关闭启动时的自动检查。

再次运行公共安装命令也受支持。

## 卸载

以下命令会删除公共安装程序创建的受管独立安装。它们会保留位于 `.openinterpreter` 下的用户数据，包括您的配置、会话、日志以及文件存储的凭证，以便您在重新安装时不丢失这些数据。

<Tabs>
  <Tab title="macOS">
    ```bash
    for name in interpreter i codex-code-mode-host; do
      path="$HOME/.local/bin/$name"
      case "$(readlink "$path" 2>/dev/null || true)" in
        "$HOME/.openinterpreter/packages/standalone/"*) rm -f "$path" ;;
      esac
    done
    rm -rf "$HOME/.openinterpreter/packages/standalone"
    ```

    如果安装程序在 `~/.zprofile` 或 `~/.bash_profile` 中添加了标记为 `Open Interpreter installer` 的块，您可以将其删除。即使保留 `~/.local/bin` 在 `PATH` 中也是安全的，特别是当其他工具使用它时。
  </Tab>
  <Tab title="Linux">
    ```bash
    for name in interpreter i codex-code-mode-host; do
      path="$HOME/.local/bin/$name"
      case "$(readlink "$path" 2>/dev/null || true)" in
        "$HOME/.openinterpreter/packages/standalone/"*) rm -f "$path" ;;
      esac
    done
    rm -rf "$HOME/.openinterpreter/packages/standalone"
    ```

    如果安装程序在 `~/.bashrc`、`~/.zshrc` 或 `~/.profile` 中添加了标记为 `Open Interpreter installer` 的块，您可以将其删除。即使保留 `~/.local/bin` 在 `PATH` 中也是安全的，尤其是当其他工具使用它时。
  </Tab>
  <Tab title="Windows PowerShell">
    ```powershell
    $binDir = Join-Path $env:LOCALAPPDATA "Programs\Open Interpreter\bin"
    $interpreterHome = Join-Path $env:USERPROFILE ".openinterpreter"
    $standaloneRoot = Join-Path $interpreterHome "packages\standalone"

    if (Test-Path -LiteralPath $binDir) {
        $binItem = Get-Item -LiteralPath $binDir -Force
        $binTarget = [string]$binItem.Target
        $isManagedJunction =
            ($binItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -and
            $binTarget.StartsWith($standaloneRoot, [StringComparison]::OrdinalIgnoreCase)

        if (-not $isManagedJunction) {
            throw "Refusing to remove $binDir because it is not an Open Interpreter managed junction."
        }

        Remove-Item -LiteralPath $binDir -Recurse -Force
    }

    Remove-Item -LiteralPath $standaloneRoot -Recurse -Force -ErrorAction SilentlyContinue

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not [string]::IsNullOrWhiteSpace($userPath)) {
        $nextPath = ($userPath -split ";" | Where-Object {
            -not [string]::Equals($_.TrimEnd("\"), $binDir.TrimEnd("\"), [StringComparison]::OrdinalIgnoreCase)
        }) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $nextPath, "User")
    }
    ```

    卸载后请打开新的 PowerShell 窗口，以使其获取更新后的用户 `PATH`。
  </Tab>
</Tabs>

如果您在安装时设置了 `OPEN_INTERPRETER_INSTALL_DIR`、`INTERPRETER_HOME`、`CODEX_INSTALL_DIR` 或 `CODEX_HOME`，请将这些自定义位置替换为上述默认路径。

<Warning>
上述命令有意保留了您的用户数据。若要同时删除本地配置、会话、日志和文件存储的凭证，请在备份完所有需要的内容后删除 macOS 或 Linux 上的 `~/.openinterpreter`，或 Windows 上的 `$env:USERPROFILE\.openinterpreter`。此操作不可撤销。它不会删除存放在操作系统密钥环或环境变量中的凭证。
</Warning>

## 从源码构建

用于本地产品开发时，可使用仓库脚本构建发行版包：

```bash
./scripts/build-interpreter-release.sh
```

不要依赖临时的 `cargo build` 来替代发行版包。该脚本会构建并放置与公共安装程序相同的受管包布局，包括入口点、打包资源以及平台帮助程序，这些都是安装上下文检测和自更新所依赖的。

## 日志

交互式 TUI 会将日志写入以下目录：

```text
~/.openinterpreter/log/
```

如需单次运行时覆盖日志目录：

```bash
interpreter -c log_dir='"./.interpreter-log"'
```

调试时可使用 `RUST_LOG` 进行 Rust 日志过滤：

```bash
RUST_LOG=info interpreter
```
