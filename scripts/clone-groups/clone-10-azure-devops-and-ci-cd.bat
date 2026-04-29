@echo off
REM ======================================================================
REM Group 10: AZURE DEVOPS & CI/CD
REM Pipelines, boards, repos, artifacts, and test plans — use together when building or automating end-to-end software delivery on Azure DevOps.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-devops-and-ci-cd

echo.
echo ======================================================================
echo  Group 10: AZURE DEVOPS & CI/CD
echo  Target: %TARGET%
echo  Repos:  7
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-devops-docs" (
    echo Cloning azure-devops-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-devops-docs.git "%TARGET%\azure-devops-docs"
) else (
    echo SKIP ^(exists^): azure-devops-docs
)

if not exist "%TARGET%\azure-devops-server-docs" (
    echo Cloning azure-devops-server-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-devops-server-docs.git "%TARGET%\azure-devops-server-docs"
) else (
    echo SKIP ^(exists^): azure-devops-server-docs
)

if not exist "%TARGET%\azure-devops-docs-sdk-web" (
    echo Cloning azure-devops-docs-sdk-web...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-devops-docs-sdk-web.git "%TARGET%\azure-devops-docs-sdk-web"
) else (
    echo SKIP ^(exists^): azure-devops-docs-sdk-web
)

if not exist "%TARGET%\azure-devops-docs-sdk-dotnet" (
    echo Cloning azure-devops-docs-sdk-dotnet...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-devops-docs-sdk-dotnet.git "%TARGET%\azure-devops-docs-sdk-dotnet"
) else (
    echo SKIP ^(exists^): azure-devops-docs-sdk-dotnet
)

if not exist "%TARGET%\azure-devops-docs-samples" (
    echo Cloning azure-devops-docs-samples...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-devops-docs-samples.git "%TARGET%\azure-devops-docs-samples"
) else (
    echo SKIP ^(exists^): azure-devops-docs-samples
)

if not exist "%TARGET%\vsts-rest-api-specs" (
    echo Cloning vsts-rest-api-specs...
    git clone --depth 1 https://github.com/MicrosoftDocs/vsts-rest-api-specs.git "%TARGET%\vsts-rest-api-specs"
) else (
    echo SKIP ^(exists^): vsts-rest-api-specs
)

if not exist "%TARGET%\azure-devops-yaml-schema" (
    echo Cloning azure-devops-yaml-schema...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-devops-yaml-schema.git "%TARGET%\azure-devops-yaml-schema"
) else (
    echo SKIP ^(exists^): azure-devops-yaml-schema
)

echo.
echo Done — Group 10 complete.
