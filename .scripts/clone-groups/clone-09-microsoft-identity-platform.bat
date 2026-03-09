@echo off
REM ======================================================================
REM Group 9: MICROSOFT IDENTITY PLATFORM
REM Authentication libraries, Entra ID (Azure AD), and identity protocols across all major languages — reference together for any auth/authz implementation.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\microsoft-identity-platform

echo.
echo ======================================================================
echo  Group 9: MICROSOFT IDENTITY PLATFORM
echo  Target: %TARGET%
echo  Repos:  3
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\entra-docs" (
    echo Cloning entra-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/entra-docs.git "%TARGET%\entra-docs"
) else (
    echo SKIP ^(exists^): entra-docs
)

if not exist "%TARGET%\entra-powershell-docs" (
    echo Cloning entra-powershell-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/entra-powershell-docs.git "%TARGET%\entra-powershell-docs"
) else (
    echo SKIP ^(exists^): entra-powershell-docs
)

if not exist "%TARGET%\MIMDocs" (
    echo Cloning MIMDocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/MIMDocs.git "%TARGET%\MIMDocs"
) else (
    echo SKIP ^(exists^): MIMDocs
)

echo.
echo Done — Group 9 complete.
