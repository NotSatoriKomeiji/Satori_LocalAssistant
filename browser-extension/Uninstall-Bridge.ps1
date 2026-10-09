$ErrorActionPreference='Stop'
foreach($Browser in @('Google\Chrome','Microsoft\Edge')){
 $Key="HKCU:\Software\$Browser\NativeMessagingHosts\io.satori.browser"
 if(Test-Path -LiteralPath $Key){Remove-Item -LiteralPath $Key}
}
$Manifest=Join-Path $PSScriptRoot 'native-host.json'
if(Test-Path -LiteralPath $Manifest){Remove-Item -LiteralPath $Manifest}
Write-Host 'Bridge removed. Remove the Satori extension from your browser separately.'
