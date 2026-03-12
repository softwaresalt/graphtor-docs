@echo off
REM ======================================================================
REM Group 32: QUANTUM COMPUTING
REM Azure Quantum service, Q# language, and quantum algorithm documentation — reference together when developing quantum programs or learning the platform.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\quantum-computing

echo.
echo ======================================================================
echo  Group 32: QUANTUM COMPUTING
echo  Target: %TARGET%
echo  Repos:  3
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\quantum-docs" (
    echo Cloning quantum-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/quantum-docs.git "%TARGET%\quantum-docs"
) else (
    echo SKIP ^(exists^): quantum-docs
)

if not exist "%TARGET%\quantum-docs-pr" (
    echo Cloning quantum-docs-pr...
    git clone --depth 1 https://github.com/MicrosoftDocs/quantum-docs-pr.git "%TARGET%\quantum-docs-pr"
) else (
    echo SKIP ^(exists^): quantum-docs-pr
)

if not exist "%TARGET%\quantum-api" (
    echo Cloning quantum-api...
    git clone --depth 1 https://github.com/MicrosoftDocs/quantum-api.git "%TARGET%\quantum-api"
) else (
    echo SKIP ^(exists^): quantum-api
)

echo.
echo Done — Group 32 complete.
