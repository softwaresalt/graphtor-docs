@echo off
REM ======================================================================
REM Group 25: VISUAL STUDIO & DEVELOPER TOOLS
REM IDE features, IntelliCode, performance tracing, package management, and dev containers — reference together during the inner development loop.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\visual-studio-and-developer-tools

echo.
echo ======================================================================
echo  Group 25: VISUAL STUDIO & DEVELOPER TOOLS
echo  Target: %TARGET%
echo  Repos:  10
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\visualstudio-docs" (
    echo Cloning visualstudio-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/visualstudio-docs.git "%TARGET%\visualstudio-docs"
) else (
    echo SKIP ^(exists^): visualstudio-docs
)

if not exist "%TARGET%\intellicode" (
    echo Cloning intellicode...
    git clone --depth 1 https://github.com/MicrosoftDocs/intellicode.git "%TARGET%\intellicode"
) else (
    echo SKIP ^(exists^): intellicode
)

if not exist "%TARGET%\vs-tutorials" (
    echo Cloning vs-tutorials...
    git clone --depth 1 https://github.com/MicrosoftDocs/vs-tutorials.git "%TARGET%\vs-tutorials"
) else (
    echo SKIP ^(exists^): vs-tutorials
)

if not exist "%TARGET%\trace-processor-docs" (
    echo Cloning trace-processor-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/trace-processor-docs.git "%TARGET%\trace-processor-docs"
) else (
    echo SKIP ^(exists^): trace-processor-docs
)

if not exist "%TARGET%\trace-processor-reference-docs" (
    echo Cloning trace-processor-reference-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/trace-processor-reference-docs.git "%TARGET%\trace-processor-reference-docs"
) else (
    echo SKIP ^(exists^): trace-processor-reference-docs
)

if not exist "%TARGET%\vcpkg-docs" (
    echo Cloning vcpkg-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/vcpkg-docs.git "%TARGET%\vcpkg-docs"
) else (
    echo SKIP ^(exists^): vcpkg-docs
)

if not exist "%TARGET%\devcontainers" (
    echo Cloning devcontainers...
    git clone --depth 1 https://github.com/MicrosoftDocs/devcontainers.git "%TARGET%\devcontainers"
) else (
    echo SKIP ^(exists^): devcontainers
)

if not exist "%TARGET%\edge-developer" (
    echo Cloning edge-developer...
    git clone --depth 1 https://github.com/MicrosoftDocs/edge-developer.git "%TARGET%\edge-developer"
) else (
    echo SKIP ^(exists^): edge-developer
)

if not exist "%TARGET%\edge-archive" (
    echo Cloning edge-archive...
    git clone --depth 1 https://github.com/MicrosoftDocs/edge-archive.git "%TARGET%\edge-archive"
) else (
    echo SKIP ^(exists^): edge-archive
)

if not exist "%TARGET%\edge-modules" (
    echo Cloning edge-modules...
    git clone --depth 1 https://github.com/MicrosoftDocs/edge-modules.git "%TARGET%\edge-modules"
) else (
    echo SKIP ^(exists^): edge-modules
)

echo.
echo Done — Group 25 complete.
