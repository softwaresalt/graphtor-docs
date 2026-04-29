@echo off
REM ======================================================================
REM Group 18: MICROSOFT GRAPH
REM Microsoft Graph API and PowerShell SDK for accessing M365, Azure AD, and cross-service data — reference together for any Graph API integration.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\microsoft-graph

echo.
echo ======================================================================
echo  Group 18: MICROSOFT GRAPH
echo  Target: %TARGET%
echo  Repos:  2
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\microsoft-graph-training" (
    echo Cloning microsoft-graph-training...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-graph-training.git "%TARGET%\microsoft-graph-training"
) else (
    echo SKIP ^(exists^): microsoft-graph-training
)

if not exist "%TARGET%\microsoftgraph-docs-powershell" (
    echo Cloning microsoftgraph-docs-powershell...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoftgraph-docs-powershell.git "%TARGET%\microsoftgraph-docs-powershell"
) else (
    echo SKIP ^(exists^): microsoftgraph-docs-powershell
)

echo.
echo Done — Group 18 complete.
