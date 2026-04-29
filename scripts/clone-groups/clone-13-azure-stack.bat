@echo off
REM ======================================================================
REM Group 13: AZURE STACK
REM Azure Stack Hub and tools for hybrid cloud scenarios where workloads run on-premises with Azure-consistent APIs.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-stack

echo.
echo ======================================================================
echo  Group 13: AZURE STACK
echo  Target: %TARGET%
echo  Repos:  1
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-stack-docs" (
    echo Cloning azure-stack-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-stack-docs.git "%TARGET%\azure-stack-docs"
) else (
    echo SKIP ^(exists^): azure-stack-docs
)

echo.
echo Done — Group 13 complete.
