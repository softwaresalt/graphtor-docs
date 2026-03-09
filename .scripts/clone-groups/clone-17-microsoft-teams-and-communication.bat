@echo off
REM ======================================================================
REM Group 17: MICROSOFT TEAMS & COMMUNICATION
REM Teams platform SDK, bots, messaging extensions, and communication services — reference together when developing Teams apps or communication workflows.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\microsoft-teams-and-communication

echo.
echo ======================================================================
echo  Group 17: MICROSOFT TEAMS & COMMUNICATION
echo  Target: %TARGET%
echo  Repos:  6
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\msteams-docs" (
    echo Cloning msteams-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/msteams-docs.git "%TARGET%\msteams-docs"
) else (
    echo SKIP ^(exists^): msteams-docs
)

if not exist "%TARGET%\Microsoft-teams-docs" (
    echo Cloning Microsoft-teams-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/Microsoft-teams-docs.git "%TARGET%\Microsoft-teams-docs"
) else (
    echo SKIP ^(exists^): Microsoft-teams-docs
)

if not exist "%TARGET%\kaizala-docs" (
    echo Cloning kaizala-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/kaizala-docs.git "%TARGET%\kaizala-docs"
) else (
    echo SKIP ^(exists^): kaizala-docs
)

if not exist "%TARGET%\teams-ai-library" (
    echo Cloning teams-ai-library...
    git clone --depth 1 https://github.com/MicrosoftDocs/teams-ai-library.git "%TARGET%\teams-ai-library"
) else (
    echo SKIP ^(exists^): teams-ai-library
)

if not exist "%TARGET%\teams-sdk-typescript" (
    echo Cloning teams-sdk-typescript...
    git clone --depth 1 https://github.com/MicrosoftDocs/teams-sdk-typescript.git "%TARGET%\teams-sdk-typescript"
) else (
    echo SKIP ^(exists^): teams-sdk-typescript
)

if not exist "%TARGET%\teams-sdk-python" (
    echo Cloning teams-sdk-python...
    git clone --depth 1 https://github.com/MicrosoftDocs/teams-sdk-python.git "%TARGET%\teams-sdk-python"
) else (
    echo SKIP ^(exists^): teams-sdk-python
)

echo.
echo Done — Group 17 complete.
