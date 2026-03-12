@echo off
REM ======================================================================
REM Group 26: .NET, C++ & LANGUAGE ECOSYSTEM
REM .NET framework/runtime, C++, F#, EF Core, Aspire, and IoT libraries — reference together when building or maintaining .NET applications.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\net-cpp-and-language-ecosystem

echo.
echo ======================================================================
echo  Group 26: .NET, C++ & LANGUAGE ECOSYSTEM
echo  Target: %TARGET%
echo  Repos:  7
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\cpp-docs" (
    echo Cloning cpp-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/cpp-docs.git "%TARGET%\cpp-docs"
) else (
    echo SKIP ^(exists^): cpp-docs
)

if not exist "%TARGET%\visualfsharpdocs" (
    echo Cloning visualfsharpdocs...
    git clone --depth 1 https://github.com/MicrosoftDocs/visualfsharpdocs.git "%TARGET%\visualfsharpdocs"
) else (
    echo SKIP ^(exists^): visualfsharpdocs
)

if not exist "%TARGET%\dotnet-archive" (
    echo Cloning dotnet-archive...
    git clone --depth 1 https://github.com/MicrosoftDocs/dotnet-archive.git "%TARGET%\dotnet-archive"
) else (
    echo SKIP ^(exists^): dotnet-archive
)

if not exist "%TARGET%\dotnet-iot-for-beginners" (
    echo Cloning dotnet-iot-for-beginners...
    git clone --depth 1 https://github.com/MicrosoftDocs/dotnet-iot-for-beginners.git "%TARGET%\dotnet-iot-for-beginners"
) else (
    echo SKIP ^(exists^): dotnet-iot-for-beginners
)

if not exist "%TARGET%\dotnet-data-for-beginners" (
    echo Cloning dotnet-data-for-beginners...
    git clone --depth 1 https://github.com/MicrosoftDocs/dotnet-data-for-beginners.git "%TARGET%\dotnet-data-for-beginners"
) else (
    echo SKIP ^(exists^): dotnet-data-for-beginners
)

if not exist "%TARGET%\node-essentials" (
    echo Cloning node-essentials...
    git clone --depth 1 https://github.com/MicrosoftDocs/node-essentials.git "%TARGET%\node-essentials"
) else (
    echo SKIP ^(exists^): node-essentials
)

if not exist "%TARGET%\aspire-docs-samples" (
    echo Cloning aspire-docs-samples...
    git clone --depth 1 https://github.com/MicrosoftDocs/aspire-docs-samples.git "%TARGET%\aspire-docs-samples"
) else (
    echo SKIP ^(exists^): aspire-docs-samples
)

echo.
echo Done — Group 26 complete.
