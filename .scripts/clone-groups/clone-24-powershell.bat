@echo off
REM ======================================================================
REM Group 24: POWERSHELL
REM PowerShell language, module gallery, DSC, and scripting documentation across Windows and Linux — reference together for automation and scripting tasks.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\powershell

echo.
echo ======================================================================
echo  Group 24: POWERSHELL
echo  Target: %TARGET%
echo  Repos:  9
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\PowerShell-Docs" (
    echo Cloning PowerShell-Docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/PowerShell-Docs.git "%TARGET%\PowerShell-Docs"
) else (
    echo SKIP ^(exists^): PowerShell-Docs
)

if not exist "%TARGET%\PowerShell-Docs-archive" (
    echo Cloning PowerShell-Docs-archive...
    git clone --depth 1 https://github.com/MicrosoftDocs/PowerShell-Docs-archive.git "%TARGET%\PowerShell-Docs-archive"
) else (
    echo SKIP ^(exists^): PowerShell-Docs-archive
)

if not exist "%TARGET%\PowerShell-Docs-Modules" (
    echo Cloning PowerShell-Docs-Modules...
    git clone --depth 1 https://github.com/MicrosoftDocs/PowerShell-Docs-Modules.git "%TARGET%\PowerShell-Docs-Modules"
) else (
    echo SKIP ^(exists^): PowerShell-Docs-Modules
)

if not exist "%TARGET%\PowerShell-Docs-DSC" (
    echo Cloning PowerShell-Docs-DSC...
    git clone --depth 1 https://github.com/MicrosoftDocs/PowerShell-Docs-DSC.git "%TARGET%\PowerShell-Docs-DSC"
) else (
    echo SKIP ^(exists^): PowerShell-Docs-DSC
)

if not exist "%TARGET%\PowerShell-Docs-PSGet" (
    echo Cloning PowerShell-Docs-PSGet...
    git clone --depth 1 https://github.com/MicrosoftDocs/PowerShell-Docs-PSGet.git "%TARGET%\PowerShell-Docs-PSGet"
) else (
    echo SKIP ^(exists^): PowerShell-Docs-PSGet
)

if not exist "%TARGET%\windows-powershell-docs" (
    echo Cloning windows-powershell-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-powershell-docs.git "%TARGET%\windows-powershell-docs"
) else (
    echo SKIP ^(exists^): windows-powershell-docs
)

if not exist "%TARGET%\PowerShell-DSC-for-Linux" (
    echo Cloning PowerShell-DSC-for-Linux...
    git clone --depth 1 https://github.com/MicrosoftDocs/PowerShell-DSC-for-Linux.git "%TARGET%\PowerShell-DSC-for-Linux"
) else (
    echo SKIP ^(exists^): PowerShell-DSC-for-Linux
)

if not exist "%TARGET%\secmgmt-open-powershell-docs" (
    echo Cloning secmgmt-open-powershell-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/secmgmt-open-powershell-docs.git "%TARGET%\secmgmt-open-powershell-docs"
) else (
    echo SKIP ^(exists^): secmgmt-open-powershell-docs
)

if not exist "%TARGET%\powerbi-docs-powershell" (
    echo Cloning powerbi-docs-powershell...
    git clone --depth 1 https://github.com/MicrosoftDocs/powerbi-docs-powershell.git "%TARGET%\powerbi-docs-powershell"
) else (
    echo SKIP ^(exists^): powerbi-docs-powershell
)

echo.
echo Done — Group 24 complete.
