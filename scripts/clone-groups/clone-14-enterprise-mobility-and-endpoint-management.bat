@echo off
REM ======================================================================
REM Group 14: ENTERPRISE MOBILITY & ENDPOINT MANAGEMENT
REM Intune, Configuration Manager, and endpoint compliance — reference together when managing and securing enterprise devices and mobile deployments.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\enterprise-mobility-and-endpoint-management

echo.
echo ======================================================================
echo  Group 14: ENTERPRISE MOBILITY & ENDPOINT MANAGEMENT
echo  Target: %TARGET%
echo  Repos:  5
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\memdocs" (
    echo Cloning memdocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/memdocs.git "%TARGET%\memdocs"
) else (
    echo SKIP ^(exists^): memdocs
)

if not exist "%TARGET%\EMDocs" (
    echo Cloning EMDocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/EMDocs.git "%TARGET%\EMDocs"
) else (
    echo SKIP ^(exists^): EMDocs
)

if not exist "%TARGET%\SCCMdocs" (
    echo Cloning SCCMdocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/SCCMdocs.git "%TARGET%\SCCMdocs"
) else (
    echo SKIP ^(exists^): SCCMdocs
)

if not exist "%TARGET%\fslogix-docs" (
    echo Cloning fslogix-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/fslogix-docs.git "%TARGET%\fslogix-docs"
) else (
    echo SKIP ^(exists^): fslogix-docs
)

if not exist "%TARGET%\Intune-and-Entra-RHEL-Private-Preview" (
    echo Cloning Intune-and-Entra-RHEL-Private-Preview...
    git clone --depth 1 https://github.com/MicrosoftDocs/Intune-and-Entra-RHEL-Private-Preview.git "%TARGET%\Intune-and-Entra-RHEL-Private-Preview"
) else (
    echo SKIP ^(exists^): Intune-and-Entra-RHEL-Private-Preview
)

echo.
echo Done — Group 14 complete.
