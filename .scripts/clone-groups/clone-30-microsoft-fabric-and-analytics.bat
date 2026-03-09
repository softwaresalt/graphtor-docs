@echo off
REM ======================================================================
REM Group 30: MICROSOFT FABRIC & ANALYTICS
REM Microsoft Fabric, Analysis Services shared components, and data visualization — reference together for building enterprise analytics and BI solutions.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\microsoft-fabric-and-analytics

echo.
echo ======================================================================
echo  Group 30: MICROSOFT FABRIC & ANALYTICS
echo  Target: %TARGET%
echo  Repos:  2
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\fabric-docs" (
    echo Cloning fabric-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/fabric-docs.git "%TARGET%\fabric-docs"
) else (
    echo SKIP ^(exists^): fabric-docs
)

if not exist "%TARGET%\bi-shared-docs" (
    echo Cloning bi-shared-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/bi-shared-docs.git "%TARGET%\bi-shared-docs"
) else (
    echo SKIP ^(exists^): bi-shared-docs
)

echo.
echo Done — Group 30 complete.
