@echo off
REM ======================================================================
REM Group 33: BING, MAPS & ADVERTISING
REM Bing Search APIs, Bing Maps, and Microsoft Advertising (Xandr) — reference together when building search, location, or programmatic advertising features.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\bing-maps-and-advertising

echo.
echo ======================================================================
echo  Group 33: BING, MAPS & ADVERTISING
echo  Target: %TARGET%
echo  Repos:  5
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\bing-docs" (
    echo Cloning bing-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/bing-docs.git "%TARGET%\bing-docs"
) else (
    echo SKIP ^(exists^): bing-docs
)

if not exist "%TARGET%\bingmaps-docs" (
    echo Cloning bingmaps-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/bingmaps-docs.git "%TARGET%\bingmaps-docs"
) else (
    echo SKIP ^(exists^): bingmaps-docs
)

if not exist "%TARGET%\Advertising-docs" (
    echo Cloning Advertising-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/Advertising-docs.git "%TARGET%\Advertising-docs"
) else (
    echo SKIP ^(exists^): Advertising-docs
)

if not exist "%TARGET%\Advertising" (
    echo Cloning Advertising...
    git clone --depth 1 https://github.com/MicrosoftDocs/Advertising.git "%TARGET%\Advertising"
) else (
    echo SKIP ^(exists^): Advertising
)

if not exist "%TARGET%\Xandr-docs" (
    echo Cloning Xandr-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/Xandr-docs.git "%TARGET%\Xandr-docs"
) else (
    echo SKIP ^(exists^): Xandr-docs
)

echo.
echo Done — Group 33 complete.
