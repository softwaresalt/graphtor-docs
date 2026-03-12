@echo off
REM ======================================================================
REM Group 34: PARTNER & PROGRAMS
REM Microsoft Partner Center, licensing, MPN programs, and collaboration portal — reference together when building partner-integrated apps or managing commercial marketplace offerings.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\partner-and-programs

echo.
echo ======================================================================
echo  Group 34: PARTNER & PROGRAMS
echo  Target: %TARGET%
echo  Repos:  4
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\partner-rest" (
    echo Cloning partner-rest...
    git clone --depth 1 https://github.com/MicrosoftDocs/partner-rest.git "%TARGET%\partner-rest"
) else (
    echo SKIP ^(exists^): partner-rest
)

if not exist "%TARGET%\partner-center-downloads" (
    echo Cloning partner-center-downloads...
    git clone --depth 1 https://github.com/MicrosoftDocs/partner-center-downloads.git "%TARGET%\partner-center-downloads"
) else (
    echo SKIP ^(exists^): partner-center-downloads
)

if not exist "%TARGET%\MicrosoftCollaboratePortal" (
    echo Cloning MicrosoftCollaboratePortal...
    git clone --depth 1 https://github.com/MicrosoftDocs/MicrosoftCollaboratePortal.git "%TARGET%\MicrosoftCollaboratePortal"
) else (
    echo SKIP ^(exists^): MicrosoftCollaboratePortal
)

if not exist "%TARGET%\intelligent-asset-manager" (
    echo Cloning intelligent-asset-manager...
    git clone --depth 1 https://github.com/MicrosoftDocs/intelligent-asset-manager.git "%TARGET%\intelligent-asset-manager"
) else (
    echo SKIP ^(exists^): intelligent-asset-manager
)

echo.
echo Done — Group 34 complete.
