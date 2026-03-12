@echo off
REM ======================================================================
REM Group 35: OPEN STANDARDS & APIS
REM OData protocol, OpenAPI specifications, and interoperability standards — reference together when designing or consuming standards-based REST APIs.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\open-standards-and-apis

echo.
echo ======================================================================
echo  Group 35: OPEN STANDARDS & APIS
echo  Target: %TARGET%
echo  Repos:  1
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\openapi-docs" (
    echo Cloning openapi-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/openapi-docs.git "%TARGET%\openapi-docs"
) else (
    echo SKIP ^(exists^): openapi-docs
)

echo.
echo Done — Group 35 complete.
