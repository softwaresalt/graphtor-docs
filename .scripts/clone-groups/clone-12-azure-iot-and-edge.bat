@echo off
REM ======================================================================
REM Group 12: AZURE IOT & EDGE
REM IoT Hub, IoT Edge, Azure Sphere, and digital twins — reference together when designing connected device solutions or edge computing workloads.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-iot-and-edge

echo.
echo ======================================================================
echo  Group 12: AZURE IOT & EDGE
echo  Target: %TARGET%
echo  Repos:  4
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\windows-iotcore-docs" (
    echo Cloning windows-iotcore-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-iotcore-docs.git "%TARGET%\windows-iotcore-docs"
) else (
    echo SKIP ^(exists^): windows-iotcore-docs
)

if not exist "%TARGET%\windows-iot-public" (
    echo Cloning windows-iot-public...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-iot-public.git "%TARGET%\windows-iot-public"
) else (
    echo SKIP ^(exists^): windows-iot-public
)

if not exist "%TARGET%\azure-sphere-issues" (
    echo Cloning azure-sphere-issues...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-sphere-issues.git "%TARGET%\azure-sphere-issues"
) else (
    echo SKIP ^(exists^): azure-sphere-issues
)

if not exist "%TARGET%\licensed-hardware" (
    echo Cloning licensed-hardware...
    git clone --depth 1 https://github.com/MicrosoftDocs/licensed-hardware.git "%TARGET%\licensed-hardware"
) else (
    echo SKIP ^(exists^): licensed-hardware
)

echo.
echo Done — Group 12 complete.
