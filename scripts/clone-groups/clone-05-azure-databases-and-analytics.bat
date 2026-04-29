@echo off
REM ======================================================================
REM Group 5: AZURE DATABASES & ANALYTICS
REM Managed relational, NoSQL, graph, and analytics database services — reference together when designing data persistence or query strategies on Azure.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-databases-and-analytics

echo.
echo ======================================================================
echo  Group 5: AZURE DATABASES & ANALYTICS
echo  Target: %TARGET%
echo  Repos:  5
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-databases-docs" (
    echo Cloning azure-databases-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-databases-docs.git "%TARGET%\azure-databases-docs"
) else (
    echo SKIP ^(exists^): azure-databases-docs
)

if not exist "%TARGET%\dataexplorer-docs" (
    echo Cloning dataexplorer-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/dataexplorer-docs.git "%TARGET%\dataexplorer-docs"
) else (
    echo SKIP ^(exists^): dataexplorer-docs
)

if not exist "%TARGET%\data-api-builder-docs" (
    echo Cloning data-api-builder-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/data-api-builder-docs.git "%TARGET%\data-api-builder-docs"
) else (
    echo SKIP ^(exists^): data-api-builder-docs
)

if not exist "%TARGET%\nosql-query-docs" (
    echo Cloning nosql-query-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/nosql-query-docs.git "%TARGET%\nosql-query-docs"
) else (
    echo SKIP ^(exists^): nosql-query-docs
)

if not exist "%TARGET%\OData-docs" (
    echo Cloning OData-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/OData-docs.git "%TARGET%\OData-docs"
) else (
    echo SKIP ^(exists^): OData-docs
)

echo.
echo Done — Group 5 complete.
