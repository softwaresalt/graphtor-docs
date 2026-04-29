@echo off
REM ======================================================================
REM Group 4: AZURE COMPUTE & INFRASTRUCTURE
REM Virtual machines, containers, Kubernetes, HPC, and compute primitives — use together when provisioning or managing Azure compute resources.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-compute-and-infrastructure

echo.
echo ======================================================================
echo  Group 4: AZURE COMPUTE & INFRASTRUCTURE
echo  Target: %TARGET%
echo  Repos:  3
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-compute-docs" (
    echo Cloning azure-compute-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-compute-docs.git "%TARGET%\azure-compute-docs"
) else (
    echo SKIP ^(exists^): azure-compute-docs
)

if not exist "%TARGET%\Virtualization-Documentation" (
    echo Cloning Virtualization-Documentation...
    git clone --depth 1 https://github.com/MicrosoftDocs/Virtualization-Documentation.git "%TARGET%\Virtualization-Documentation"
) else (
    echo SKIP ^(exists^): Virtualization-Documentation
)

if not exist "%TARGET%\azure-aks-docs" (
    echo Cloning azure-aks-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-aks-docs.git "%TARGET%\azure-aks-docs"
) else (
    echo SKIP ^(exists^): azure-aks-docs
)

echo.
echo Done — Group 4 complete.
