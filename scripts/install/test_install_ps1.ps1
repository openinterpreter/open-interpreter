Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$installerPath = Join-Path $PSScriptRoot "install.ps1"
$tokens = $null
$parseErrors = $null
$installerAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $installerPath,
    [ref]$tokens,
    [ref]$parseErrors
)
if ($parseErrors.Count -ne 0) {
    throw "install.ps1 did not parse: $($parseErrors -join '; ')"
}

foreach ($functionName in @("Resolve-WindowsArchitecture", "Get-WindowsRuntimeArchitecture")) {
    $functionAst = $installerAst.Find(
        {
            param($node)
            $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
                $node.Name -eq $functionName
        },
        $true
    )
    if ($null -eq $functionAst) {
        throw "Could not find $functionName in install.ps1"
    }
    Invoke-Expression $functionAst.Extent.Text
}

function Assert-Equal {
    param(
        [object]$Actual,
        [object]$Expected,
        [string]$Case
    )

    if ($Actual -cne $Expected) {
        throw "$Case expected '$Expected' but got '$Actual'"
    }
}

Assert-Equal `
    (Resolve-WindowsArchitecture "X64" "ARM64" "AMD64") `
    "X64" `
    "runtime architecture takes precedence"
Assert-Equal `
    (Resolve-WindowsArchitecture $null "ARM64" "AMD64") `
    "Arm64" `
    "WOW64 reports the native ARM64 architecture"
Assert-Equal `
    (Resolve-WindowsArchitecture $null $null "AMD64") `
    "X64" `
    "process architecture is the final fallback"
Assert-Equal `
    (Resolve-WindowsArchitecture $null $null "x64") `
    "X64" `
    "x64 alias is accepted"

$unsupportedFailed = $false
try {
    Resolve-WindowsArchitecture $null $null "x86"
} catch {
    $unsupportedFailed = $_.Exception.Message -eq "Unsupported architecture: x86"
}
if (-not $unsupportedFailed) {
    throw "unsupported architectures must fail explicitly"
}

$runtimeArchitecture = Get-WindowsRuntimeArchitecture
if ($runtimeArchitecture -notin @("X64", "Arm64")) {
    throw "unexpected runtime architecture on Windows: $runtimeArchitecture"
}

Write-Host "install.ps1 architecture tests passed for $runtimeArchitecture"
