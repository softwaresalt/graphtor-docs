@echo off
REM ======================================================================
REM Group 16: MICROSOFT 365 & OFFICE
REM Productivity applications and developer APIs covering Word, Excel, Outlook, SharePoint, Exchange, and Lync/UCMA — used together for M365 app development or administrative automation.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\microsoft-365-and-office

echo.
echo ======================================================================
echo  Group 16: MICROSOFT 365 & OFFICE
echo  Target: %TARGET%
echo  Repos:  12
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\microsoft-365-docs" (
    echo Cloning microsoft-365-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-365-docs.git "%TARGET%\microsoft-365-docs"
) else (
    echo SKIP ^(exists^): microsoft-365-docs
)

if not exist "%TARGET%\office-docs-powershell" (
    echo Cloning office-docs-powershell...
    git clone --depth 1 https://github.com/MicrosoftDocs/office-docs-powershell.git "%TARGET%\office-docs-powershell"
) else (
    echo SKIP ^(exists^): office-docs-powershell
)

if not exist "%TARGET%\office-developer-client-docs" (
    echo Cloning office-developer-client-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/office-developer-client-docs.git "%TARGET%\office-developer-client-docs"
) else (
    echo SKIP ^(exists^): office-developer-client-docs
)

if not exist "%TARGET%\office-developer-exchange-docs" (
    echo Cloning office-developer-exchange-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/office-developer-exchange-docs.git "%TARGET%\office-developer-exchange-docs"
) else (
    echo SKIP ^(exists^): office-developer-exchange-docs
)

if not exist "%TARGET%\office-developer-lync-evergreen-docs" (
    echo Cloning office-developer-lync-evergreen-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/office-developer-lync-evergreen-docs.git "%TARGET%\office-developer-lync-evergreen-docs"
) else (
    echo SKIP ^(exists^): office-developer-lync-evergreen-docs
)

if not exist "%TARGET%\office-developer-msproject-xml-docs" (
    echo Cloning office-developer-msproject-xml-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs.git "%TARGET%\office-developer-msproject-xml-docs"
) else (
    echo SKIP ^(exists^): office-developer-msproject-xml-docs
)

if not exist "%TARGET%\office-365-management-api" (
    echo Cloning office-365-management-api...
    git clone --depth 1 https://github.com/MicrosoftDocs/office-365-management-api.git "%TARGET%\office-365-management-api"
) else (
    echo SKIP ^(exists^): office-365-management-api
)

if not exist "%TARGET%\OfficeDocs-SharePoint-PowerShell" (
    echo Cloning OfficeDocs-SharePoint-PowerShell...
    git clone --depth 1 https://github.com/MicrosoftDocs/OfficeDocs-SharePoint-PowerShell.git "%TARGET%\OfficeDocs-SharePoint-PowerShell"
) else (
    echo SKIP ^(exists^): OfficeDocs-SharePoint-PowerShell
)

if not exist "%TARGET%\VBA-Docs" (
    echo Cloning VBA-Docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/VBA-Docs.git "%TARGET%\VBA-Docs"
) else (
    echo SKIP ^(exists^): VBA-Docs
)

if not exist "%TARGET%\microsoft-365-community" (
    echo Cloning microsoft-365-community...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-365-community.git "%TARGET%\microsoft-365-community"
) else (
    echo SKIP ^(exists^): microsoft-365-community
)

if not exist "%TARGET%\oufr-dev-docs" (
    echo Cloning oufr-dev-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/oufr-dev-docs.git "%TARGET%\oufr-dev-docs"
) else (
    echo SKIP ^(exists^): oufr-dev-docs
)

if not exist "%TARGET%\microsoft-community-training" (
    echo Cloning microsoft-community-training...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-community-training.git "%TARGET%\microsoft-community-training"
) else (
    echo SKIP ^(exists^): microsoft-community-training
)

echo.
echo Done — Group 16 complete.
