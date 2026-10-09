$ErrorActionPreference='Stop'
$ExtensionId=(Get-Content -LiteralPath (Join-Path $PSScriptRoot 'extension-id.txt') -Raw).Trim()
if($ExtensionId -notmatch '^[a-p]{32}$'){throw 'Invalid extension ID'}
$Executable=Join-Path (Split-Path $PSScriptRoot -Parent) 'satori.exe'
if(!(Test-Path -LiteralPath $Executable)){throw 'Place browser-extension beside satori.exe before installing'}
$Manifest=Join-Path $PSScriptRoot 'native-host.json'
$Data=@{name='io.satori.browser';description='Satori local website event bridge';path=$Executable;type='stdio';allowed_origins=@("chrome-extension://$ExtensionId/")}
$Json=$Data|ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($Manifest,$Json,[System.Text.UTF8Encoding]::new($false))
foreach($Browser in @('Google\Chrome','Microsoft\Edge')){
 $Key="HKCU:\Software\$Browser\NativeMessagingHosts\io.satori.browser"
 New-Item -Path $Key -Force|Out-Null
 Set-Item -Path $Key -Value $Manifest
}
Write-Host 'Bridge installed for current user. Load this folder as an unpacked extension in Chrome/Edge, then turn on Quick Words in Satori. Website observation is optional.'
