@echo off
REM ======================================================================
REM Group 1: AZURE CORE & CLI
REM The primary Azure documentation hub plus CLI, PowerShell, and REST API reference — the baseline layer any Azure user or developer needs first.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-core-and-cli

echo.
echo ======================================================================
echo  Group 1: AZURE CORE & CLI
echo  Target: %TARGET%
echo  Repos:  6
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-docs" (
    echo Cloning azure-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs.git "%TARGET%\azure-docs"
) else (
    echo SKIP ^(exists^): azure-docs
)

if not exist "%TARGET%\azure-dev-docs" (
    echo Cloning azure-dev-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-dev-docs.git "%TARGET%\azure-dev-docs"
) else (
    echo SKIP ^(exists^): azure-dev-docs
)

if not exist "%TARGET%\azure-docs-cli" (
    echo Cloning azure-docs-cli...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-cli.git "%TARGET%\azure-docs-cli"
) else (
    echo SKIP ^(exists^): azure-docs-cli
)

if not exist "%TARGET%\azure-docs-powershell" (
    echo Cloning azure-docs-powershell...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-powershell.git "%TARGET%\azure-docs-powershell"
) else (
    echo SKIP ^(exists^): azure-docs-powershell
)

if not exist "%TARGET%\azure-reference-other" (
    echo Cloning azure-reference-other...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-reference-other.git "%TARGET%\azure-reference-other"
) else (
    echo SKIP ^(exists^): azure-reference-other
)

if not exist "%TARGET%\SupportArticles-docs" (
    echo Cloning SupportArticles-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/SupportArticles-docs.git "%TARGET%\SupportArticles-docs"
) else (
    echo SKIP ^(exists^): SupportArticles-docs
)

echo.
echo Done — Group 1 complete.
