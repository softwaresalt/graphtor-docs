@echo off
REM ======================================================================
REM Group 27: MOBILE DEVELOPMENT
REM Xamarin, .NET MAUI, App Center, and mobile Blazor — reference together when building cross-platform mobile applications targeting iOS and Android.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\mobile-development

echo.
echo ======================================================================
echo  Group 27: MOBILE DEVELOPMENT
echo  Target: %TARGET%
echo  Repos:  5
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\xamarin-docs" (
    echo Cloning xamarin-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/xamarin-docs.git "%TARGET%\xamarin-docs"
) else (
    echo SKIP ^(exists^): xamarin-docs
)

if not exist "%TARGET%\xamarin-communitytoolkit" (
    echo Cloning xamarin-communitytoolkit...
    git clone --depth 1 https://github.com/MicrosoftDocs/xamarin-communitytoolkit.git "%TARGET%\xamarin-communitytoolkit"
) else (
    echo SKIP ^(exists^): xamarin-communitytoolkit
)

if not exist "%TARGET%\appcenter-docs" (
    echo Cloning appcenter-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/appcenter-docs.git "%TARGET%\appcenter-docs"
) else (
    echo SKIP ^(exists^): appcenter-docs
)

if not exist "%TARGET%\mobile-blazor-bindings" (
    echo Cloning mobile-blazor-bindings...
    git clone --depth 1 https://github.com/MicrosoftDocs/mobile-blazor-bindings.git "%TARGET%\mobile-blazor-bindings"
) else (
    echo SKIP ^(exists^): mobile-blazor-bindings
)

if not exist "%TARGET%\CordovaDocs" (
    echo Cloning CordovaDocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/CordovaDocs.git "%TARGET%\CordovaDocs"
) else (
    echo SKIP ^(exists^): CordovaDocs
)

echo.
echo Done — Group 27 complete.
