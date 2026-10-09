# Runs the production decoder without enabling AI or sending provider requests.
$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot '../src-tauri/scripts/ai.ps1'
$source = [IO.File]::ReadAllText($scriptPath, [Text.Encoding]::UTF8)
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'AI adapter has a PowerShell syntax error' }
$functions = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] }, $false)
foreach ($function in $functions) { Invoke-Expression $function.Extent.Text }
$utf8 = [Text.UTF8Encoding]::new($false, $true)
$checks = 0
function Assert-Equal($actual, $expected, $name) {
    if ($actual -cne $expected) { throw "FAILED: $name" }
    $script:checks++
}
function Assert-Rejected([byte[]]$bytes, $name) {
    $rejected = $false
    try { $null = ConvertFrom-SatoriAiResponse -Bytes $bytes } catch { $rejected = $true }
    if (!$rejected) { throw "FAILED: $name was accepted" }
    $script:checks++
}
function Response-Bytes([string]$text) {
    $json = @{choices=@(@{message=@{content=$text}})} | ConvertTo-Json -Depth 5 -Compress
    return $utf8.GetBytes($json)
}
# ASCII-only test source also works when Windows PowerShell reads .ps1 as ANSI.
$chinese = -join [char[]]@(0x4F60, 0x597D, 0xFF0C, 0x8FD9, 0x662F, 0x4E2D, 0x6587, 0x3002)
$expected = $chinese + "`nLine 2: caf" + [char]0xE9 + ' ' + [char]::ConvertFromUtf32(0x1F642)
$bytes = Response-Bytes $expected
Assert-Equal (ConvertFrom-SatoriAiResponse -Bytes $bytes) $expected 'literal UTF-8 Chinese, accents, emoji and newline'
$bomBytes = [byte[]](@(0xEF, 0xBB, 0xBF) + $bytes)
Assert-Equal (ConvertFrom-SatoriAiResponse -Bytes $bomBytes) $expected 'UTF-8 BOM'
$escaped = '{"choices":[{"message":{"content":"\u4f60\u597d\nOK"}}]}'
Assert-Equal (ConvertFrom-SatoriAiResponse -Bytes $utf8.GetBytes($escaped)) ((-join [char[]]@(0x4F60,0x597D))+"`nOK") 'escaped JSON Unicode'
$advice = @{interpretation='keep_scene';app_id=$null;explanation=$chinese} | ConvertTo-Json -Compress
Assert-Equal (ConvertFrom-SatoriAiResponse -Bytes (Response-Bytes $advice)) $advice 'cold-learning JSON remains unchanged'
Assert-Rejected $utf8.GetBytes('{"choices":[]}') 'empty choices'
Assert-Rejected $utf8.GetBytes('{"choices":[{"message":{"content":null}}]}') 'null content'
Assert-Rejected $utf8.GetBytes('{"choices":[{"message":{"content":["text"]}}]}') 'non-string content'
Assert-Rejected (Response-Bytes '   ') 'blank content'
Assert-Rejected $utf8.GetBytes('{broken json') 'invalid JSON'
Assert-Rejected ([byte[]]@(0x7B,0xFF,0x7D)) 'invalid UTF-8'
Assert-Rejected (Response-Bytes ('a' * 32769)) 'text character limit'
Assert-Rejected (Response-Bytes (([string][char]0x4F60) * 30000)) 'UTF-8 pipe byte limit'
Assert-Rejected ([byte[]]::new(1048577)) 'HTTP body limit'
# Simulate the old response-decoding failure and prove why Latin-1 is unsuitable.
$latin1 = [Text.Encoding]::GetEncoding(28591).GetString($bytes) | ConvertFrom-Json
if ($latin1.choices[0].message.content -ceq $expected) { throw 'Mojibake reproduction failed' }
$checks++
Write-Output "AI response decoding: $checks checks passed (no provider requests)."
