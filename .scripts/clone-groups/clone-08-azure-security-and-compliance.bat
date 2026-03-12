@echo off
REM ======================================================================
REM Group 8: AZURE SECURITY & COMPLIANCE
REM Threat protection, security benchmarks, information protection, and compliance tooling — use together when hardening Azure workloads or meeting audit needs.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-security-and-compliance

echo.
echo ======================================================================
echo  Group 8: AZURE SECURITY & COMPLIANCE
echo  Target: %TARGET%
echo  Repos:  13
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-security-docs" (
    echo Cloning azure-security-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-security-docs.git "%TARGET%\azure-security-docs"
) else (
    echo SKIP ^(exists^): azure-security-docs
)

if not exist "%TARGET%\SecurityBenchmarks" (
    echo Cloning SecurityBenchmarks...
    git clone --depth 1 https://github.com/MicrosoftDocs/SecurityBenchmarks.git "%TARGET%\SecurityBenchmarks"
) else (
    echo SKIP ^(exists^): SecurityBenchmarks
)

if not exist "%TARGET%\defender-docs" (
    echo Cloning defender-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/defender-docs.git "%TARGET%\defender-docs"
) else (
    echo SKIP ^(exists^): defender-docs
)

if not exist "%TARGET%\security" (
    echo Cloning security...
    git clone --depth 1 https://github.com/MicrosoftDocs/security.git "%TARGET%\security"
) else (
    echo SKIP ^(exists^): security
)

if not exist "%TARGET%\security-services-docs" (
    echo Cloning security-services-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/security-services-docs.git "%TARGET%\security-services-docs"
) else (
    echo SKIP ^(exists^): security-services-docs
)

if not exist "%TARGET%\ATADocs" (
    echo Cloning ATADocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/ATADocs.git "%TARGET%\ATADocs"
) else (
    echo SKIP ^(exists^): ATADocs
)

if not exist "%TARGET%\Azure-RMSDocs" (
    echo Cloning Azure-RMSDocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/Azure-RMSDocs.git "%TARGET%\Azure-RMSDocs"
) else (
    echo SKIP ^(exists^): Azure-RMSDocs
)

if not exist "%TARGET%\CloudAppSecurityDocs" (
    echo Cloning CloudAppSecurityDocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/CloudAppSecurityDocs.git "%TARGET%\CloudAppSecurityDocs"
) else (
    echo SKIP ^(exists^): CloudAppSecurityDocs
)

if not exist "%TARGET%\mip-sdk-docs" (
    echo Cloning mip-sdk-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/mip-sdk-docs.git "%TARGET%\mip-sdk-docs"
) else (
    echo SKIP ^(exists^): mip-sdk-docs
)

if not exist "%TARGET%\Linux-compliance-private-preview" (
    echo Cloning Linux-compliance-private-preview...
    git clone --depth 1 https://github.com/MicrosoftDocs/Linux-compliance-private-preview.git "%TARGET%\Linux-compliance-private-preview"
) else (
    echo SKIP ^(exists^): Linux-compliance-private-preview
)

if not exist "%TARGET%\Azure-Trusted-Launch-VMs-Linux" (
    echo Cloning Azure-Trusted-Launch-VMs-Linux...
    git clone --depth 1 https://github.com/MicrosoftDocs/Azure-Trusted-Launch-VMs-Linux.git "%TARGET%\Azure-Trusted-Launch-VMs-Linux"
) else (
    echo SKIP ^(exists^): Azure-Trusted-Launch-VMs-Linux
)

if not exist "%TARGET%\Backup-Confidential-VMs-with-CMK" (
    echo Cloning Backup-Confidential-VMs-with-CMK...
    git clone --depth 1 https://github.com/MicrosoftDocs/Backup-Confidential-VMs-with-CMK.git "%TARGET%\Backup-Confidential-VMs-with-CMK"
) else (
    echo SKIP ^(exists^): Backup-Confidential-VMs-with-CMK
)

if not exist "%TARGET%\secured-core-pc" (
    echo Cloning secured-core-pc...
    git clone --depth 1 https://github.com/MicrosoftDocs/secured-core-pc.git "%TARGET%\secured-core-pc"
) else (
    echo SKIP ^(exists^): secured-core-pc
)

echo.
echo Done — Group 8 complete.
