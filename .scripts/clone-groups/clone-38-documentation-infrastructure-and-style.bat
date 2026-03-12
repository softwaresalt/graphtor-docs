@echo off
REM ======================================================================
REM Group 38: DOCUMENTATION INFRASTRUCTURE & STYLE
REM Authoring guides, style guide, templates, tooling, and contribution workflows — reference together when contributing to or maintaining Microsoft docs.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\documentation-infrastructure-and-style

echo.
echo ======================================================================
echo  Group 38: DOCUMENTATION INFRASTRUCTURE & STYLE
echo  Target: %TARGET%
echo  Repos:  15
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\Contribute" (
    echo Cloning Contribute...
    git clone --depth 1 https://github.com/MicrosoftDocs/Contribute.git "%TARGET%\Contribute"
) else (
    echo SKIP ^(exists^): Contribute
)

if not exist "%TARGET%\microsoft-style-guide" (
    echo Cloning microsoft-style-guide...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-style-guide.git "%TARGET%\microsoft-style-guide"
) else (
    echo SKIP ^(exists^): microsoft-style-guide
)

if not exist "%TARGET%\content-templates" (
    echo Cloning content-templates...
    git clone --depth 1 https://github.com/MicrosoftDocs/content-templates.git "%TARGET%\content-templates"
) else (
    echo SKIP ^(exists^): content-templates
)

if not exist "%TARGET%\DocsContentNav" (
    echo Cloning DocsContentNav...
    git clone --depth 1 https://github.com/MicrosoftDocs/DocsContentNav.git "%TARGET%\DocsContentNav"
) else (
    echo SKIP ^(exists^): DocsContentNav
)

if not exist "%TARGET%\learn-template" (
    echo Cloning learn-template...
    git clone --depth 1 https://github.com/MicrosoftDocs/learn-template.git "%TARGET%\learn-template"
) else (
    echo SKIP ^(exists^): learn-template
)

if not exist "%TARGET%\learn-scaffolding" (
    echo Cloning learn-scaffolding...
    git clone --depth 1 https://github.com/MicrosoftDocs/learn-scaffolding.git "%TARGET%\learn-scaffolding"
) else (
    echo SKIP ^(exists^): learn-scaffolding
)

if not exist "%TARGET%\sphinx-docfx-yaml-docs" (
    echo Cloning sphinx-docfx-yaml-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/sphinx-docfx-yaml-docs.git "%TARGET%\sphinx-docfx-yaml-docs"
) else (
    echo SKIP ^(exists^): sphinx-docfx-yaml-docs
)

if not exist "%TARGET%\rest-ref-template" (
    echo Cloning rest-ref-template...
    git clone --depth 1 https://github.com/MicrosoftDocs/rest-ref-template.git "%TARGET%\rest-ref-template"
) else (
    echo SKIP ^(exists^): rest-ref-template
)

if not exist "%TARGET%\java-ref-template" (
    echo Cloning java-ref-template...
    git clone --depth 1 https://github.com/MicrosoftDocs/java-ref-template.git "%TARGET%\java-ref-template"
) else (
    echo SKIP ^(exists^): java-ref-template
)

if not exist "%TARGET%\ObjectiveC-ref-template" (
    echo Cloning ObjectiveC-ref-template...
    git clone --depth 1 https://github.com/MicrosoftDocs/ObjectiveC-ref-template.git "%TARGET%\ObjectiveC-ref-template"
) else (
    echo SKIP ^(exists^): ObjectiveC-ref-template
)

if not exist "%TARGET%\archive-template" (
    echo Cloning archive-template...
    git clone --depth 1 https://github.com/MicrosoftDocs/archive-template.git "%TARGET%\archive-template"
) else (
    echo SKIP ^(exists^): archive-template
)

if not exist "%TARGET%\swa-template" (
    echo Cloning swa-template...
    git clone --depth 1 https://github.com/MicrosoftDocs/swa-template.git "%TARGET%\swa-template"
) else (
    echo SKIP ^(exists^): swa-template
)

if not exist "%TARGET%\azure-docs-pr-template" (
    echo Cloning azure-docs-pr-template...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-pr-template.git "%TARGET%\azure-docs-pr-template"
) else (
    echo SKIP ^(exists^): azure-docs-pr-template
)

if not exist "%TARGET%\executable-docs" (
    echo Cloning executable-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/executable-docs.git "%TARGET%\executable-docs"
) else (
    echo SKIP ^(exists^): executable-docs
)

if not exist "%TARGET%\typography-issues" (
    echo Cloning typography-issues...
    git clone --depth 1 https://github.com/MicrosoftDocs/typography-issues.git "%TARGET%\typography-issues"
) else (
    echo SKIP ^(exists^): typography-issues
)

echo.
echo Done — Group 38 complete.
