$ErrorActionPreference = "Continue"
$vcvarsall = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat"
$tempFile = [System.IO.Path]::GetTempFileName()
cmd /c "`"$vcvarsall`" x64 && set" > $tempFile 2>&1
Get-Content $tempFile | ForEach-Object {
    if ($_ -match "^([^=]+)=(.*)$") {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
}
Remove-Item $tempFile
Set-Location C:\Users\cobra\axiom
$output = cargo build 2>&1
$output | Out-File -FilePath C:\Users\cobra\axiom\build_output.txt -Encoding utf8
"EXIT_CODE=$LASTEXITCODE" | Add-Content -Path C:\Users\cobra\axiom\build_output.txt
