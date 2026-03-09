@echo off
REM ======================================================================
REM Group 36: MIGRATION TOOLS
REM Database migration, app containerization, and cloud migration guidance — reference together during lift-and-shift or modernization projects.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\migration-tools

echo.
echo ======================================================================
echo  Group 36: MIGRATION TOOLS
echo  Target: %TARGET%
echo  Repos:  1
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\MigrationPlaybookContent" (
    echo Cloning MigrationPlaybookContent...
    git clone --depth 1 https://github.com/MicrosoftDocs/MigrationPlaybookContent.git "%TARGET%\MigrationPlaybookContent"
) else (
    echo SKIP ^(exists^): MigrationPlaybookContent
)

echo.
echo Done — Group 36 complete.
