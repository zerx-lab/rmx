$ErrorActionPreference = 'Continue'
$proc = Start-Process -FilePath 'C:\Users\zero\Desktop\code\rmx\target\release\rmx.exe' -ArgumentList '-fv','--stats','--kill-processes','C:\Users\zero\Desktop\123.xlsx' -NoNewWindow -Wait -PassThru -RedirectStandardOutput 'C:\Users\zero\Desktop\code\rmx\stdout.txt' -RedirectStandardError 'C:\Users\zero\Desktop\code\rmx\stderr.txt'
Write-Output "EXIT: $($proc.ExitCode)"
Write-Output "--- STDOUT ---"
Get-Content 'C:\Users\zero\Desktop\code\rmx\stdout.txt'
Write-Output "--- STDERR ---"
Get-Content 'C:\Users\zero\Desktop\code\rmx\stderr.txt'
Write-Output "--- FILE EXISTS ---"
Test-Path 'C:\Users\zero\Desktop\123.xlsx'
