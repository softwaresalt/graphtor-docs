@echo off
REM ======================================================================
REM Group 29: POWER PLATFORM
REM Power Apps, Power Automate, Power BI, Power Pages, Power Query, and AI Builder — reference together when building low-code/no-code solutions or BI reports.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\power-platform

echo.
echo ======================================================================
echo  Group 29: POWER PLATFORM
echo  Target: %TARGET%
echo  Repos:  10
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\power-platform" (
    echo Cloning power-platform...
    git clone --depth 1 https://github.com/MicrosoftDocs/power-platform.git "%TARGET%\power-platform"
) else (
    echo SKIP ^(exists^): power-platform
)

if not exist "%TARGET%\powerapps-docs" (
    echo Cloning powerapps-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/powerapps-docs.git "%TARGET%\powerapps-docs"
) else (
    echo SKIP ^(exists^): powerapps-docs
)

if not exist "%TARGET%\power-automate-docs" (
    echo Cloning power-automate-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/power-automate-docs.git "%TARGET%\power-automate-docs"
) else (
    echo SKIP ^(exists^): power-automate-docs
)

if not exist "%TARGET%\powerbi-docs" (
    echo Cloning powerbi-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/powerbi-docs.git "%TARGET%\powerbi-docs"
) else (
    echo SKIP ^(exists^): powerbi-docs
)

if not exist "%TARGET%\power-pages-docs" (
    echo Cloning power-pages-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/power-pages-docs.git "%TARGET%\power-pages-docs"
) else (
    echo SKIP ^(exists^): power-pages-docs
)

if not exist "%TARGET%\ai-builder" (
    echo Cloning ai-builder...
    git clone --depth 1 https://github.com/MicrosoftDocs/ai-builder.git "%TARGET%\ai-builder"
) else (
    echo SKIP ^(exists^): ai-builder
)

if not exist "%TARGET%\data-tools" (
    echo Cloning data-tools...
    git clone --depth 1 https://github.com/MicrosoftDocs/data-tools.git "%TARGET%\data-tools"
) else (
    echo SKIP ^(exists^): data-tools
)

if not exist "%TARGET%\query-docs" (
    echo Cloning query-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/query-docs.git "%TARGET%\query-docs"
) else (
    echo SKIP ^(exists^): query-docs
)

if not exist "%TARGET%\data-integration" (
    echo Cloning data-integration...
    git clone --depth 1 https://github.com/MicrosoftDocs/data-integration.git "%TARGET%\data-integration"
) else (
    echo SKIP ^(exists^): data-integration
)

if not exist "%TARGET%\powerquery-docs" (
    echo Cloning powerquery-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/powerquery-docs.git "%TARGET%\powerquery-docs"
) else (
    echo SKIP ^(exists^): powerquery-docs
)

echo.
echo Done — Group 29 complete.
