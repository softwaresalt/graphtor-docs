@echo off
REM ======================================================================
REM Group 31: AZURE SDK LANGUAGE REFERENCES
REM Per-language SDK API references for .NET, Java, Python, JavaScript/TypeScript, Go, and C++ — use the language-specific repo alongside azure-docs when writing application code against Azure services.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-sdk-language-references

echo.
echo ======================================================================
echo  Group 31: AZURE SDK LANGUAGE REFERENCES
echo  Target: %TARGET%
echo  Repos:  9
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-docs-sdk-dotnet" (
    echo Cloning azure-docs-sdk-dotnet...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-dotnet.git "%TARGET%\azure-docs-sdk-dotnet"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-dotnet
)

if not exist "%TARGET%\azure-docs-sdk-java" (
    echo Cloning azure-docs-sdk-java...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-java.git "%TARGET%\azure-docs-sdk-java"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-java
)

if not exist "%TARGET%\azure-docs-sdk-python" (
    echo Cloning azure-docs-sdk-python...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-python.git "%TARGET%\azure-docs-sdk-python"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-python
)

if not exist "%TARGET%\azure-docs-sdk-python-archive" (
    echo Cloning azure-docs-sdk-python-archive...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-python-archive.git "%TARGET%\azure-docs-sdk-python-archive"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-python-archive
)

if not exist "%TARGET%\azure-docs-sdk-node" (
    echo Cloning azure-docs-sdk-node...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-node.git "%TARGET%\azure-docs-sdk-node"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-node
)

if not exist "%TARGET%\azure-docs-sdk-node-archive" (
    echo Cloning azure-docs-sdk-node-archive...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-node-archive.git "%TARGET%\azure-docs-sdk-node-archive"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-node-archive
)

if not exist "%TARGET%\azure-docs-sdk-go" (
    echo Cloning azure-docs-sdk-go...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-go.git "%TARGET%\azure-docs-sdk-go"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-go
)

if not exist "%TARGET%\azure-docs-sdk-cpp" (
    echo Cloning azure-docs-sdk-cpp...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-docs-sdk-cpp.git "%TARGET%\azure-docs-sdk-cpp"
) else (
    echo SKIP ^(exists^): azure-docs-sdk-cpp
)

if not exist "%TARGET%\azure-java-reference" (
    echo Cloning azure-java-reference...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-java-reference.git "%TARGET%\azure-java-reference"
) else (
    echo SKIP ^(exists^): azure-java-reference
)

echo.
echo Done — Group 31 complete.
