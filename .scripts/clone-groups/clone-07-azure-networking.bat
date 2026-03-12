@echo off
REM ======================================================================
REM Group 7: AZURE NETWORKING
REM Virtual networks, load balancers, traffic management, DNS, and VPN gateways — reference together for network topology, routing, and connectivity design.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-networking

echo.
echo ======================================================================
echo  Group 7: AZURE NETWORKING
echo  Target: %TARGET%
echo  Repos:  1
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\PreviewContentforNetworkWatcher" (
    echo Cloning PreviewContentforNetworkWatcher...
    git clone --depth 1 https://github.com/MicrosoftDocs/PreviewContentforNetworkWatcher.git "%TARGET%\PreviewContentforNetworkWatcher"
) else (
    echo SKIP ^(exists^): PreviewContentforNetworkWatcher
)

echo.
echo Done — Group 7 complete.
