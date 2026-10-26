# Read-only prerequisite for tests that send real Windows keyboard input.
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class InputDesktopCheck {
 [DllImport("user32.dll",SetLastError=true)]public static extern IntPtr OpenInputDesktop(uint flags,bool inherit,uint access);
 [DllImport("user32.dll")]public static extern bool CloseDesktop(IntPtr desktop);
}
'@
$desktop = [InputDesktopCheck]::OpenInputDesktop(0,$false,1)
$nativeError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
if ($desktop -ne [IntPtr]::Zero) { [void][InputDesktopCheck]::CloseDesktop($desktop) }
$available = $desktop -ne [IntPtr]::Zero
@{available=$available;nativeError=$(if($available){0}else{$nativeError})} | ConvertTo-Json -Compress
if (!$available) {
 [Console]::Error.WriteLine('Interactive Windows desktop unavailable. Unlock Windows before testing native keyboard input.')
 exit 2
}
