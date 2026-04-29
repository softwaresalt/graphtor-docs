@echo off
REM ======================================================================
REM Group 22: WINDOWS DEVELOPMENT
REM Win32, WinRT, WinUI, WebView2, console, UWP tools, and the Windows Community Toolkit — reference together when building native Windows applications.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\windows-development

echo.
echo ======================================================================
echo  Group 22: WINDOWS DEVELOPMENT
echo  Target: %TARGET%
echo  Repos:  22
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\windows-dev-docs" (
    echo Cloning windows-dev-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-dev-docs.git "%TARGET%\windows-dev-docs"
) else (
    echo SKIP ^(exists^): windows-dev-docs
)

if not exist "%TARGET%\windows-driver-docs" (
    echo Cloning windows-driver-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-driver-docs.git "%TARGET%\windows-driver-docs"
) else (
    echo SKIP ^(exists^): windows-driver-docs
)

if not exist "%TARGET%\windows-driver-docs-ddi" (
    echo Cloning windows-driver-docs-ddi...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-driver-docs-ddi.git "%TARGET%\windows-driver-docs-ddi"
) else (
    echo SKIP ^(exists^): windows-driver-docs-ddi
)

if not exist "%TARGET%\win32" (
    echo Cloning win32...
    git clone --depth 1 https://github.com/MicrosoftDocs/win32.git "%TARGET%\win32"
) else (
    echo SKIP ^(exists^): win32
)

if not exist "%TARGET%\winrt-api" (
    echo Cloning winrt-api...
    git clone --depth 1 https://github.com/MicrosoftDocs/winrt-api.git "%TARGET%\winrt-api"
) else (
    echo SKIP ^(exists^): winrt-api
)

if not exist "%TARGET%\winrt-related" (
    echo Cloning winrt-related...
    git clone --depth 1 https://github.com/MicrosoftDocs/winrt-related.git "%TARGET%\winrt-related"
) else (
    echo SKIP ^(exists^): winrt-related
)

if not exist "%TARGET%\sdk-api" (
    echo Cloning sdk-api...
    git clone --depth 1 https://github.com/MicrosoftDocs/sdk-api.git "%TARGET%\sdk-api"
) else (
    echo SKIP ^(exists^): sdk-api
)

if not exist "%TARGET%\winui-api" (
    echo Cloning winui-api...
    git clone --depth 1 https://github.com/MicrosoftDocs/winui-api.git "%TARGET%\winui-api"
) else (
    echo SKIP ^(exists^): winui-api
)

if not exist "%TARGET%\winapps-winrt-api" (
    echo Cloning winapps-winrt-api...
    git clone --depth 1 https://github.com/MicrosoftDocs/winapps-winrt-api.git "%TARGET%\winapps-winrt-api"
) else (
    echo SKIP ^(exists^): winapps-winrt-api
)

if not exist "%TARGET%\winapps-win32-api" (
    echo Cloning winapps-win32-api...
    git clone --depth 1 https://github.com/MicrosoftDocs/winapps-win32-api.git "%TARGET%\winapps-win32-api"
) else (
    echo SKIP ^(exists^): winapps-win32-api
)

if not exist "%TARGET%\Console-Docs" (
    echo Cloning Console-Docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/Console-Docs.git "%TARGET%\Console-Docs"
) else (
    echo SKIP ^(exists^): Console-Docs
)

if not exist "%TARGET%\WSL" (
    echo Cloning WSL...
    git clone --depth 1 https://github.com/MicrosoftDocs/WSL.git "%TARGET%\WSL"
) else (
    echo SKIP ^(exists^): WSL
)

if not exist "%TARGET%\globalization" (
    echo Cloning globalization...
    git clone --depth 1 https://github.com/MicrosoftDocs/globalization.git "%TARGET%\globalization"
) else (
    echo SKIP ^(exists^): globalization
)

if not exist "%TARGET%\sysinternals" (
    echo Cloning sysinternals...
    git clone --depth 1 https://github.com/MicrosoftDocs/sysinternals.git "%TARGET%\sysinternals"
) else (
    echo SKIP ^(exists^): sysinternals
)

if not exist "%TARGET%\terminal" (
    echo Cloning terminal...
    git clone --depth 1 https://github.com/MicrosoftDocs/terminal.git "%TARGET%\terminal"
) else (
    echo SKIP ^(exists^): terminal
)

if not exist "%TARGET%\msix-docs" (
    echo Cloning msix-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/msix-docs.git "%TARGET%\msix-docs"
) else (
    echo SKIP ^(exists^): msix-docs
)

if not exist "%TARGET%\WindowsCommunityToolkitDocs" (
    echo Cloning WindowsCommunityToolkitDocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/WindowsCommunityToolkitDocs.git "%TARGET%\WindowsCommunityToolkitDocs"
) else (
    echo SKIP ^(exists^): WindowsCommunityToolkitDocs
)

if not exist "%TARGET%\CommunityToolkit" (
    echo Cloning CommunityToolkit...
    git clone --depth 1 https://github.com/MicrosoftDocs/CommunityToolkit.git "%TARGET%\CommunityToolkit"
) else (
    echo SKIP ^(exists^): CommunityToolkit
)

if not exist "%TARGET%\windows-devdocs-team" (
    echo Cloning windows-devdocs-team...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-devdocs-team.git "%TARGET%\windows-devdocs-team"
) else (
    echo SKIP ^(exists^): windows-devdocs-team
)

if not exist "%TARGET%\cross-device" (
    echo Cloning cross-device...
    git clone --depth 1 https://github.com/MicrosoftDocs/cross-device.git "%TARGET%\cross-device"
) else (
    echo SKIP ^(exists^): cross-device
)

if not exist "%TARGET%\gestures-docs" (
    echo Cloning gestures-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/gestures-docs.git "%TARGET%\gestures-docs"
) else (
    echo SKIP ^(exists^): gestures-docs
)

if not exist "%TARGET%\MS-AppControl-AppManifests" (
    echo Cloning MS-AppControl-AppManifests...
    git clone --depth 1 https://github.com/MicrosoftDocs/MS-AppControl-AppManifests.git "%TARGET%\MS-AppControl-AppManifests"
) else (
    echo SKIP ^(exists^): MS-AppControl-AppManifests
)

echo.
echo Done — Group 22 complete.
