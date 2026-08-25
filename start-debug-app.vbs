Option Explicit

Dim shell, fileSystem, projectRoot, powershellPath, runnerPath, logPath
Dim command, exitCode

Set shell = CreateObject("WScript.Shell")
Set fileSystem = CreateObject("Scripting.FileSystemObject")

projectRoot = fileSystem.GetParentFolderName(WScript.ScriptFullName)
powershellPath = shell.ExpandEnvironmentStrings("%SystemRoot%") & _
  "\System32\WindowsPowerShell\v1.0\powershell.exe"
runnerPath = fileSystem.BuildPath(projectRoot, "tools\start_debug_app_hidden.ps1")
logPath = fileSystem.BuildPath(projectRoot, ".runtime\debug-launch.log")

If Not fileSystem.FileExists(runnerPath) Then
  MsgBox "The current-source launcher is incomplete:" & vbCrLf & runnerPath, _
    vbCritical + vbOKOnly, "Atsumi Next - Development"
  WScript.Quit 1
End If

command = Quote(powershellPath) & _
  " -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass" & _
  " -WindowStyle Hidden -File " & Quote(runnerPath)

exitCode = shell.Run(command, 0, True)

If exitCode = 73 Then
  MsgBox "The current-source app is already starting.", _
    vbInformation + vbOKOnly, "Atsumi Next - Development"
ElseIf exitCode = 74 Then
  MsgBox "Another Atsumi Next app is already running." & vbCrLf & _
    "Close it completely, including the tray, and try again.", _
    vbExclamation + vbOKOnly, "Atsumi Next - Development"
ElseIf exitCode <> 0 Then
  MsgBox "The current-source app could not be started." & vbCrLf & vbCrLf & _
    "Details were saved here:" & vbCrLf & logPath, _
    vbCritical + vbOKOnly, "Atsumi Next - Development"
End If

WScript.Quit exitCode

Function Quote(value)
  Quote = Chr(34) & Replace(value, Chr(34), Chr(34) & Chr(34)) & Chr(34)
End Function
