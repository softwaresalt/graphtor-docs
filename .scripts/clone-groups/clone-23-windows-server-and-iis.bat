@echo off
REM ======================================================================
REM Group 23: WINDOWS SERVER & IIS
REM Windows Server, IIS web server, and Linux-on-Azure documentation — reference together for server infrastructure administration and configuration.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\windows-server-and-iis

echo.
echo ======================================================================
echo  Group 23: WINDOWS SERVER & IIS
echo  Target: %TARGET%
echo  Repos:  3
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\windowsserverdocs" (
    echo Cloning windowsserverdocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/windowsserverdocs.git "%TARGET%\windowsserverdocs"
) else (
    echo SKIP ^(exists^): windowsserverdocs
)

if not exist "%TARGET%\iis-docs" (
    echo Cloning iis-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/iis-docs.git "%TARGET%\iis-docs"
) else (
    echo SKIP ^(exists^): iis-docs
)

if not exist "%TARGET%\linux" (
    echo Cloning linux...
    git clone --depth 1 https://github.com/MicrosoftDocs/linux.git "%TARGET%\linux"
) else (
    echo SKIP ^(exists^): linux
)

echo.
echo Done — Group 23 complete.
