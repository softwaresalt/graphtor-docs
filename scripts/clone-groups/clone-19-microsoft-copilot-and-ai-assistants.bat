@echo off
REM ======================================================================
REM Group 19: MICROSOFT COPILOT & AI ASSISTANTS
REM Copilot for M365, Copilot plugins, connectors, and the Model Context Protocol — reference together when building or extending Copilot-powered experiences.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\microsoft-copilot-and-ai-assistants

echo.
echo ======================================================================
echo  Group 19: MICROSOFT COPILOT & AI ASSISTANTS
echo  Target: %TARGET%
echo  Repos:  5
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\m365copilot-docs" (
    echo Cloning m365copilot-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/m365copilot-docs.git "%TARGET%\m365copilot-docs"
) else (
    echo SKIP ^(exists^): m365copilot-docs
)

if not exist "%TARGET%\copilot-connectors" (
    echo Cloning copilot-connectors...
    git clone --depth 1 https://github.com/MicrosoftDocs/copilot-connectors.git "%TARGET%\copilot-connectors"
) else (
    echo SKIP ^(exists^): copilot-connectors
)

if not exist "%TARGET%\microsoft-salescopilot-docs" (
    echo Cloning microsoft-salescopilot-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-salescopilot-docs.git "%TARGET%\microsoft-salescopilot-docs"
) else (
    echo SKIP ^(exists^): microsoft-salescopilot-docs
)

if not exist "%TARGET%\mcp" (
    echo Cloning mcp...
    git clone --depth 1 https://github.com/MicrosoftDocs/mcp.git "%TARGET%\mcp"
) else (
    echo SKIP ^(exists^): mcp
)

if not exist "%TARGET%\Agent-Skills" (
    echo Cloning Agent-Skills...
    git clone --depth 1 https://github.com/MicrosoftDocs/Agent-Skills.git "%TARGET%\Agent-Skills"
) else (
    echo SKIP ^(exists^): Agent-Skills
)

echo.
echo Done — Group 19 complete.
