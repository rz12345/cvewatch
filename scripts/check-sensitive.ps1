<#
.SYNOPSIS
  commit 前檢查暫存區新增的行是否含本機敏感字詞（硬體型號、資產編號、內部文件編號等）。

.DESCRIPTION
  讀取 <repo>\data\sensitive-patterns.txt（/data 已 gitignore，不入版控），
  逐行比對 `git diff --cached` 新增的行，並解開暫存區中的 Office 檔（xlsx／docx／pptx）比對其中 XML 的文字；
  有命中時列出「檔名:行號」（Office 檔為「檔名!內部檔名」）並以 1 結束。
  清單不存在或沒有任何有效字詞時同樣以非 0 結束，避免誤以為檢查通過。

  清單格式（UTF-8，每行一項）：
    # 開頭      註解
    空行        略過
    re:<規則>   .NET 正規表示式
    其他        一般字詞（逐字比對）
  比對一律不分大小寫。

.PARAMETER PatternFile
  改用其他清單檔（預設為 <repo>\data\sensitive-patterns.txt）。

.EXAMPLE
  git add -A; powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check-sensitive.ps1
#>
[CmdletBinding()]
param(
    [string]$PatternFile
)

$ErrorActionPreference = 'Stop'
$exitHit = 1
$exitSetup = 2

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$repoRoot = (& git rev-parse --show-toplevel 2>$null)
if ($LASTEXITCODE -ne 0 -or -not $repoRoot) {
    Write-Host '[check-sensitive] 目前目錄不在 git repo 內。' -ForegroundColor Red
    exit $exitSetup
}
$repoRoot = $repoRoot.Trim()

if (-not $PatternFile) {
    $PatternFile = Join-Path $repoRoot 'data\sensitive-patterns.txt'
}

if (-not (Test-Path -LiteralPath $PatternFile -PathType Leaf)) {
    Write-Host "[check-sensitive] 找不到敏感字詞清單：$PatternFile" -ForegroundColor Red
    Write-Host '  請建立此檔（UTF-8，每行一個字詞；re: 開頭為正規表示式；# 開頭為註解）。'
    Write-Host '  清單放在 data\ 之下，不會進入版控。'
    exit $exitSetup
}

function Read-SensitivePatterns {
    param([string]$Path)
    $result = @()
    foreach ($raw in (Get-Content -LiteralPath $Path -Encoding UTF8)) {
        $line = $raw.Trim()
        if ($line -eq '' -or $line.StartsWith('#')) { continue }
        if ($line.StartsWith('re:')) {
            $source = $line.Substring(3)
        } else {
            $source = [regex]::Escape($line)
        }
        try {
            $result += [regex]::new($source, 'IgnoreCase, CultureInvariant')
        } catch {
            Write-Host "[check-sensitive] 無效的正規表示式：$line" -ForegroundColor Red
            exit $exitSetup
        }
    }
    return , $result
}

$patterns = Read-SensitivePatterns -Path $PatternFile
if ($patterns.Count -eq 0) {
    Write-Host "[check-sensitive] 清單沒有任何字詞：$PatternFile" -ForegroundColor Red
    exit $exitSetup
}

$diff = & git -C $repoRoot -c core.quotepath=false diff --cached --no-color --no-ext-diff -U0 --diff-filter=ACMR
if ($LASTEXITCODE -ne 0) {
    Write-Host '[check-sensitive] 無法讀取暫存區的差異（git diff --cached 失敗）。' -ForegroundColor Red
    exit $exitSetup
}

$hits = @()
$file = $null
$lineNo = 0
# hunk 內剩餘的舊行數與新行數：在 hunk 內時，`+++ ` / `--- ` 開頭的是內容（新增 `++ …` 或刪除 `-- …` 的行），
# 不是檔頭
$oldLeft = 0
$newLeft = 0
foreach ($line in $diff) {
    if ($oldLeft -gt 0 -or $newLeft -gt 0) {
        if ($line.StartsWith('+')) {
            $newLeft--
            if ($file) {
                $text = $line.Substring(1)
                foreach ($p in $patterns) {
                    if ($p.IsMatch($text)) {
                        $hits += [pscustomobject]@{ File = $file; Line = $lineNo; Pattern = $p.ToString() }
                        break
                    }
                }
            }
            $lineNo++
        } elseif ($line.StartsWith('-')) {
            $oldLeft--
        } elseif ($line.StartsWith(' ')) {
            $oldLeft--
            $newLeft--
            $lineNo++
        }
        # `\ No newline at end of file` 不計入行數
        continue
    }
    if ($line.StartsWith('+++ ')) {
        $target = $line.Substring(4)
        $file = if ($target -eq '/dev/null') { $null } else { $target -replace '^b/', '' }
        continue
    }
    if ($line -match '^@@ -\d+(?:,(\d+))? \+(\d+)(?:,(\d+))? @@') {
        $oldLeft = if ($Matches[1]) { [int]$Matches[1] } else { 1 }
        $lineNo = [int]$Matches[2]
        $newLeft = if ($Matches[3]) { [int]$Matches[3] } else { 1 }
    }
}

# Office Open XML（xlsx／docx／pptx）是 zip，文字 diff 看不到內容：取出暫存區版本，逐一比對其中 XML 的文字
function Find-OfficeHits {
    param([string]$Root, [object[]]$Patterns)
    $names = & git -C $Root -c core.quotepath=false diff --cached --name-only --diff-filter=ACMR
    if ($LASTEXITCODE -ne 0) {
        Write-Host '[check-sensitive] 無法列出暫存區的檔案（git diff --cached --name-only 失敗）。' -ForegroundColor Red
        exit $exitSetup
    }
    $office = @($names | Where-Object { $_ -match '\.(xlsx|xlsm|docx|pptx)$' })
    if ($office.Count -eq 0) { return , @() }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("check-sensitive-" + [guid]::NewGuid().ToString('N'))
    $found = @()
    try {
        & git -C $Root checkout-index "--prefix=$tmp/" -- $office
        if ($LASTEXITCODE -ne 0) {
            Write-Host '[check-sensitive] 無法取出暫存區的 Office 檔（git checkout-index 失敗）。' -ForegroundColor Red
            exit $exitSetup
        }
        foreach ($name in $office) {
            $zip = $null
            try {
                $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $tmp $name))
            } catch {
                Write-Host "[check-sensitive] 無法以 zip 讀取 ${name}：$($_.Exception.Message)" -ForegroundColor Red
                exit $exitSetup
            }
            try {
                foreach ($entry in ($zip.Entries | Where-Object { $_.FullName -like '*.xml' })) {
                    $reader = New-Object IO.StreamReader($entry.Open(), [Text.Encoding]::UTF8)
                    try { $xml = $reader.ReadToEnd() } finally { $reader.Dispose() }
                    # 標籤換成換行，避免相鄰儲存格的文字黏在一起
                    $text = [regex]::Replace($xml, '<[^>]+>', "`n")
                    foreach ($p in $Patterns) {
                        if ($p.IsMatch($text)) {
                            $found += [pscustomobject]@{ File = "$name!$($entry.FullName)"; Line = '-'; Pattern = $p.ToString() }
                        }
                    }
                }
            } finally {
                $zip.Dispose()
            }
        }
    } finally {
        if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
    }
    return , $found
}

$hits += Find-OfficeHits -Root $repoRoot -Patterns $patterns

if ($hits.Count -gt 0) {
    Write-Host "[check-sensitive] 暫存區有 $($hits.Count) 行含敏感字詞：" -ForegroundColor Red
    foreach ($h in $hits) {
        Write-Host ("  {0}:{1}  （規則 {2}）" -f $h.File, $h.Line, $h.Pattern)
    }
    Write-Host '  請改成虛構寫法或移到 data\catalog.d 後再 commit。'
    exit $exitHit
}

Write-Host "[check-sensitive] 通過：暫存區新增的行沒有命中 $($patterns.Count) 項敏感字詞。" -ForegroundColor Green
exit 0
