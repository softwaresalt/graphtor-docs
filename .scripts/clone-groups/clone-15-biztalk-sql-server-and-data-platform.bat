@echo off
REM ======================================================================
REM Group 15: BIZTALK, SQL SERVER & DATA PLATFORM
REM On-premises and hybrid data platform including SQL Server, BizTalk, and Reporting Services — reference together for enterprise data integration work.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\biztalk-sql-server-and-data-platform

echo.
echo ======================================================================
echo  Group 15: BIZTALK, SQL SERVER & DATA PLATFORM
echo  Target: %TARGET%
echo  Repos:  3
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\sql-docs" (
    echo Cloning sql-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/sql-docs.git "%TARGET%\sql-docs"
) else (
    echo SKIP ^(exists^): sql-docs
)

if not exist "%TARGET%\biztalk-docs" (
    echo Cloning biztalk-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/biztalk-docs.git "%TARGET%\biztalk-docs"
) else (
    echo SKIP ^(exists^): biztalk-docs
)

if not exist "%TARGET%\Reporting-Services" (
    echo Cloning Reporting-Services...
    git clone --depth 1 https://github.com/MicrosoftDocs/Reporting-Services.git "%TARGET%\Reporting-Services"
) else (
    echo SKIP ^(exists^): Reporting-Services
)

echo.
echo Done — Group 15 complete.
