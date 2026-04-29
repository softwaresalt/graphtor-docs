@echo off
REM ======================================================================
REM Group 21: GAMING, MEDIA & MIXED REALITY
REM Xbox Live, PlayFab, Azure Gaming, HoloLens/Mesh, AltspaceVR, Minecraft, and PlayReady DRM — use together for game or immersive experience development.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\gaming-media-and-mixed-reality

echo.
echo ======================================================================
echo  Group 21: GAMING, MEDIA & MIXED REALITY
echo  Target: %TARGET%
echo  Repos:  8
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-gaming-docs" (
    echo Cloning azure-gaming-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-gaming-docs.git "%TARGET%\azure-gaming-docs"
) else (
    echo SKIP ^(exists^): azure-gaming-docs
)

if not exist "%TARGET%\xbox-live-docs" (
    echo Cloning xbox-live-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/xbox-live-docs.git "%TARGET%\xbox-live-docs"
) else (
    echo SKIP ^(exists^): xbox-live-docs
)

if not exist "%TARGET%\PlayReady" (
    echo Cloning PlayReady...
    git clone --depth 1 https://github.com/MicrosoftDocs/PlayReady.git "%TARGET%\PlayReady"
) else (
    echo SKIP ^(exists^): PlayReady
)

if not exist "%TARGET%\playfab-docs" (
    echo Cloning playfab-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/playfab-docs.git "%TARGET%\playfab-docs"
) else (
    echo SKIP ^(exists^): playfab-docs
)

if not exist "%TARGET%\minecraft-creator" (
    echo Cloning minecraft-creator...
    git clone --depth 1 https://github.com/MicrosoftDocs/minecraft-creator.git "%TARGET%\minecraft-creator"
) else (
    echo SKIP ^(exists^): minecraft-creator
)

if not exist "%TARGET%\altspace-vr" (
    echo Cloning altspace-vr...
    git clone --depth 1 https://github.com/MicrosoftDocs/altspace-vr.git "%TARGET%\altspace-vr"
) else (
    echo SKIP ^(exists^): altspace-vr
)

if not exist "%TARGET%\mesh-docs" (
    echo Cloning mesh-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/mesh-docs.git "%TARGET%\mesh-docs"
) else (
    echo SKIP ^(exists^): mesh-docs
)

if not exist "%TARGET%\azure-video-indexer" (
    echo Cloning azure-video-indexer...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-video-indexer.git "%TARGET%\azure-video-indexer"
) else (
    echo SKIP ^(exists^): azure-video-indexer
)

echo.
echo Done — Group 21 complete.
