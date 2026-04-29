@echo off
REM ======================================================================
REM Group 2: AZURE ARCHITECTURE & BEST PRACTICES
REM High-level guidance for designing, migrating, and operating cloud solutions — used together when planning or reviewing any Azure workload design.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-architecture-and-best-practices

echo.
echo ======================================================================
echo  Group 2: AZURE ARCHITECTURE & BEST PRACTICES
echo  Target: %TARGET%
echo  Repos:  5
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\architecture-center" (
    echo Cloning architecture-center...
    git clone --depth 1 https://github.com/MicrosoftDocs/architecture-center.git "%TARGET%\architecture-center"
) else (
    echo SKIP ^(exists^): architecture-center
)

if not exist "%TARGET%\cloud-adoption-framework" (
    echo Cloning cloud-adoption-framework...
    git clone --depth 1 https://github.com/MicrosoftDocs/cloud-adoption-framework.git "%TARGET%\cloud-adoption-framework"
) else (
    echo SKIP ^(exists^): cloud-adoption-framework
)

if not exist "%TARGET%\well-architected" (
    echo Cloning well-architected...
    git clone --depth 1 https://github.com/MicrosoftDocs/well-architected.git "%TARGET%\well-architected"
) else (
    echo SKIP ^(exists^): well-architected
)

if not exist "%TARGET%\patterns-practices" (
    echo Cloning patterns-practices...
    git clone --depth 1 https://github.com/MicrosoftDocs/patterns-practices.git "%TARGET%\patterns-practices"
) else (
    echo SKIP ^(exists^): patterns-practices
)

if not exist "%TARGET%\microsoft-cloud" (
    echo Cloning microsoft-cloud...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-cloud.git "%TARGET%\microsoft-cloud"
) else (
    echo SKIP ^(exists^): microsoft-cloud
)

echo.
echo Done — Group 2 complete.
