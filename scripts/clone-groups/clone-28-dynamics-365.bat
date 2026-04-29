@echo off
REM ======================================================================
REM Group 28: DYNAMICS 365
REM ERP, CRM, field service, finance, mixed reality, and business apps spanning the full Dynamics 365 product family — reference together for Dynamics dev or admin work across any of the product lines.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\dynamics-365

echo.
echo ======================================================================
echo  Group 28: DYNAMICS 365
echo  Target: %TARGET%
echo  Repos:  27
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\dynamics-365-unified-operations-public" (
    echo Cloning dynamics-365-unified-operations-public...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-unified-operations-public.git "%TARGET%\dynamics-365-unified-operations-public"
) else (
    echo SKIP ^(exists^): dynamics-365-unified-operations-public
)

if not exist "%TARGET%\dynamics365smb-docs" (
    echo Cloning dynamics365smb-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics365smb-docs.git "%TARGET%\dynamics365smb-docs"
) else (
    echo SKIP ^(exists^): dynamics365smb-docs
)

if not exist "%TARGET%\dynamics365smb-devitpro-pb" (
    echo Cloning dynamics365smb-devitpro-pb...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics365smb-devitpro-pb.git "%TARGET%\dynamics365smb-devitpro-pb"
) else (
    echo SKIP ^(exists^): dynamics365smb-devitpro-pb
)

if not exist "%TARGET%\dynamics-365-customer-engagement" (
    echo Cloning dynamics-365-customer-engagement...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-customer-engagement.git "%TARGET%\dynamics-365-customer-engagement"
) else (
    echo SKIP ^(exists^): dynamics-365-customer-engagement
)

if not exist "%TARGET%\dynamics-365-mixed-reality" (
    echo Cloning dynamics-365-mixed-reality...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-mixed-reality.git "%TARGET%\dynamics-365-mixed-reality"
) else (
    echo SKIP ^(exists^): dynamics-365-mixed-reality
)

if not exist "%TARGET%\dynamics-365-ai" (
    echo Cloning dynamics-365-ai...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-ai.git "%TARGET%\dynamics-365-ai"
) else (
    echo SKIP ^(exists^): dynamics-365-ai
)

if not exist "%TARGET%\dynamics-365-fraud-protection" (
    echo Cloning dynamics-365-fraud-protection...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-fraud-protection.git "%TARGET%\dynamics-365-fraud-protection"
) else (
    echo SKIP ^(exists^): dynamics-365-fraud-protection
)

if not exist "%TARGET%\dynamics-365-project-operations" (
    echo Cloning dynamics-365-project-operations...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-project-operations.git "%TARGET%\dynamics-365-project-operations"
) else (
    echo SKIP ^(exists^): dynamics-365-project-operations
)

if not exist "%TARGET%\dynamics-365-intelligent-order-management" (
    echo Cloning dynamics-365-intelligent-order-management...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-intelligent-order-management.git "%TARGET%\dynamics-365-intelligent-order-management"
) else (
    echo SKIP ^(exists^): dynamics-365-intelligent-order-management
)

if not exist "%TARGET%\dynamics-365-supply-chain-insights" (
    echo Cloning dynamics-365-supply-chain-insights...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-supply-chain-insights.git "%TARGET%\dynamics-365-supply-chain-insights"
) else (
    echo SKIP ^(exists^): dynamics-365-supply-chain-insights
)

if not exist "%TARGET%\dynamics-365-contact-center" (
    echo Cloning dynamics-365-contact-center...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics-365-contact-center.git "%TARGET%\dynamics-365-contact-center"
) else (
    echo SKIP ^(exists^): dynamics-365-contact-center
)

if not exist "%TARGET%\dynamics365-industry-solutions" (
    echo Cloning dynamics365-industry-solutions...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics365-industry-solutions.git "%TARGET%\dynamics365-industry-solutions"
) else (
    echo SKIP ^(exists^): dynamics365-industry-solutions
)

if not exist "%TARGET%\dynamics365-guidance" (
    echo Cloning dynamics365-guidance...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics365-guidance.git "%TARGET%\dynamics365-guidance"
) else (
    echo SKIP ^(exists^): dynamics365-guidance
)

if not exist "%TARGET%\dynamics365-docs-templates" (
    echo Cloning dynamics365-docs-templates...
    git clone --depth 1 https://github.com/MicrosoftDocs/dynamics365-docs-templates.git "%TARGET%\dynamics365-docs-templates"
) else (
    echo SKIP ^(exists^): dynamics365-docs-templates
)

if not exist "%TARGET%\DynamicsAX2012-technet" (
    echo Cloning DynamicsAX2012-technet...
    git clone --depth 1 https://github.com/MicrosoftDocs/DynamicsAX2012-technet.git "%TARGET%\DynamicsAX2012-technet"
) else (
    echo SKIP ^(exists^): DynamicsAX2012-technet
)

if not exist "%TARGET%\DynamicsAX2012-msdn" (
    echo Cloning DynamicsAX2012-msdn...
    git clone --depth 1 https://github.com/MicrosoftDocs/DynamicsAX2012-msdn.git "%TARGET%\DynamicsAX2012-msdn"
) else (
    echo SKIP ^(exists^): DynamicsAX2012-msdn
)

if not exist "%TARGET%\nav-content" (
    echo Cloning nav-content...
    git clone --depth 1 https://github.com/MicrosoftDocs/nav-content.git "%TARGET%\nav-content"
) else (
    echo SKIP ^(exists^): nav-content
)

if not exist "%TARGET%\navdevitpro-content-pr" (
    echo Cloning navdevitpro-content-pr...
    git clone --depth 1 https://github.com/MicrosoftDocs/navdevitpro-content-pr.git "%TARGET%\navdevitpro-content-pr"
) else (
    echo SKIP ^(exists^): navdevitpro-content-pr
)

if not exist "%TARGET%\msftdynamicsgpdocs" (
    echo Cloning msftdynamicsgpdocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/msftdynamicsgpdocs.git "%TARGET%\msftdynamicsgpdocs"
) else (
    echo SKIP ^(exists^): msftdynamicsgpdocs
)

if not exist "%TARGET%\d365Ops-Financials" (
    echo Cloning d365Ops-Financials...
    git clone --depth 1 https://github.com/MicrosoftDocs/d365Ops-Financials.git "%TARGET%\d365Ops-Financials"
) else (
    echo SKIP ^(exists^): d365Ops-Financials
)

if not exist "%TARGET%\customer-voice" (
    echo Cloning customer-voice...
    git clone --depth 1 https://github.com/MicrosoftDocs/customer-voice.git "%TARGET%\customer-voice"
) else (
    echo SKIP ^(exists^): customer-voice
)

if not exist "%TARGET%\customer-insights" (
    echo Cloning customer-insights...
    git clone --depth 1 https://github.com/MicrosoftDocs/customer-insights.git "%TARGET%\customer-insights"
) else (
    echo SKIP ^(exists^): customer-insights
)

if not exist "%TARGET%\supply-chain-center" (
    echo Cloning supply-chain-center...
    git clone --depth 1 https://github.com/MicrosoftDocs/supply-chain-center.git "%TARGET%\supply-chain-center"
) else (
    echo SKIP ^(exists^): supply-chain-center
)

if not exist "%TARGET%\common-data-model-and-service" (
    echo Cloning common-data-model-and-service...
    git clone --depth 1 https://github.com/MicrosoftDocs/common-data-model-and-service.git "%TARGET%\common-data-model-and-service"
) else (
    echo SKIP ^(exists^): common-data-model-and-service
)

if not exist "%TARGET%\connected-store" (
    echo Cloning connected-store...
    git clone --depth 1 https://github.com/MicrosoftDocs/connected-store.git "%TARGET%\connected-store"
) else (
    echo SKIP ^(exists^): connected-store
)

if not exist "%TARGET%\connected-spaces" (
    echo Cloning connected-spaces...
    git clone --depth 1 https://github.com/MicrosoftDocs/connected-spaces.git "%TARGET%\connected-spaces"
) else (
    echo SKIP ^(exists^): connected-spaces
)

if not exist "%TARGET%\D365FnOArchiveWithDataverseLongTermRetention" (
    echo Cloning D365FnOArchiveWithDataverseLongTermRetention...
    git clone --depth 1 https://github.com/MicrosoftDocs/D365FnOArchiveWithDataverseLongTermRetention.git "%TARGET%\D365FnOArchiveWithDataverseLongTermRetention"
) else (
    echo SKIP ^(exists^): D365FnOArchiveWithDataverseLongTermRetention
)

echo.
echo Done — Group 28 complete.
