@echo off
REM ======================================================================
REM Group 11: AZURE MONITORING & MANAGEMENT
REM Monitor, Log Analytics, System Center, and operational management — use together when setting up observability and operational runbooks.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-monitoring-and-management

echo.
echo ======================================================================
echo  Group 11: AZURE MONITORING & MANAGEMENT
echo  Target: %TARGET%
echo  Repos:  4
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-monitor-docs" (
    echo Cloning azure-monitor-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-monitor-docs.git "%TARGET%\azure-monitor-docs"
) else (
    echo SKIP ^(exists^): azure-monitor-docs
)

if not exist "%TARGET%\azure-management-docs" (
    echo Cloning azure-management-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-management-docs.git "%TARGET%\azure-management-docs"
) else (
    echo SKIP ^(exists^): azure-management-docs
)

if not exist "%TARGET%\reliability-docs" (
    echo Cloning reliability-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/reliability-docs.git "%TARGET%\reliability-docs"
) else (
    echo SKIP ^(exists^): reliability-docs
)

if not exist "%TARGET%\SystemCenterDocs" (
    echo Cloning SystemCenterDocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/SystemCenterDocs.git "%TARGET%\SystemCenterDocs"
) else (
    echo SKIP ^(exists^): SystemCenterDocs
)

echo.
echo Done — Group 11 complete.
