@echo off
REM ======================================================================
REM Group 20: BOT FRAMEWORK & CONVERSATIONAL AI
REM Bot Framework SDK across .NET, TypeScript, Python, plus Adaptive Cards and Composer — reference together when building chatbots or dialog systems.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\bot-framework-and-conversational-ai

echo.
echo ======================================================================
echo  Group 20: BOT FRAMEWORK & CONVERSATIONAL AI
echo  Target: %TARGET%
echo  Repos:  4
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\bot-docs" (
    echo Cloning bot-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/bot-docs.git "%TARGET%\bot-docs"
) else (
    echo SKIP ^(exists^): bot-docs
)

if not exist "%TARGET%\composer-docs" (
    echo Cloning composer-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/composer-docs.git "%TARGET%\composer-docs"
) else (
    echo SKIP ^(exists^): composer-docs
)

if not exist "%TARGET%\CortanaSkillsKit" (
    echo Cloning CortanaSkillsKit...
    git clone --depth 1 https://github.com/MicrosoftDocs/CortanaSkillsKit.git "%TARGET%\CortanaSkillsKit"
) else (
    echo SKIP ^(exists^): CortanaSkillsKit
)

if not exist "%TARGET%\AdaptiveCards" (
    echo Cloning AdaptiveCards...
    git clone --depth 1 https://github.com/MicrosoftDocs/AdaptiveCards.git "%TARGET%\AdaptiveCards"
) else (
    echo SKIP ^(exists^): AdaptiveCards
)

echo.
echo Done — Group 20 complete.
