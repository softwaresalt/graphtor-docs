@echo off
REM ======================================================================
REM Group 6: AZURE STORAGE
REM Blob, table, queue, and file storage — grouped so storage patterns, lifecycle management, and SDK usage can be consulted side-by-side.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-storage

echo.
echo ======================================================================
echo  Group 6: AZURE STORAGE
echo  Target: %TARGET%
echo  Repos:  1
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-storage-typescript-docs" (
    echo Cloning azure-storage-typescript-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-storage-typescript-docs.git "%TARGET%\azure-storage-typescript-docs"
) else (
    echo SKIP ^(exists^): azure-storage-typescript-docs
)

echo.
echo Done — Group 6 complete.
