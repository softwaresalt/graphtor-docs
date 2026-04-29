@echo off
REM ======================================================================
REM Group 3: AZURE AI & MACHINE LEARNING
REM AI services, cognitive APIs, ML frameworks, and speech/vision services — reference these together when building intelligent, AI-powered applications.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\azure-ai-and-machine-learning

echo.
echo ======================================================================
echo  Group 3: AZURE AI & MACHINE LEARNING
echo  Target: %TARGET%
echo  Repos:  9
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\azure-ai-docs" (
    echo Cloning azure-ai-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/azure-ai-docs.git "%TARGET%\azure-ai-docs"
) else (
    echo SKIP ^(exists^): azure-ai-docs
)

if not exist "%TARGET%\cognitive-toolkit-docs" (
    echo Cloning cognitive-toolkit-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/cognitive-toolkit-docs.git "%TARGET%\cognitive-toolkit-docs"
) else (
    echo SKIP ^(exists^): cognitive-toolkit-docs
)

if not exist "%TARGET%\machine-learning-server-docs" (
    echo Cloning machine-learning-server-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/machine-learning-server-docs.git "%TARGET%\machine-learning-server-docs"
) else (
    echo SKIP ^(exists^): machine-learning-server-docs
)

if not exist "%TARGET%\Speech" (
    echo Cloning Speech...
    git clone --depth 1 https://github.com/MicrosoftDocs/Speech.git "%TARGET%\Speech"
) else (
    echo SKIP ^(exists^): Speech
)

if not exist "%TARGET%\SpeechServicePrivatePreview" (
    echo Cloning SpeechServicePrivatePreview...
    git clone --depth 1 https://github.com/MicrosoftDocs/SpeechServicePrivatePreview.git "%TARGET%\SpeechServicePrivatePreview"
) else (
    echo SKIP ^(exists^): SpeechServicePrivatePreview
)

if not exist "%TARGET%\prose-py-api-docs" (
    echo Cloning prose-py-api-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/prose-py-api-docs.git "%TARGET%\prose-py-api-docs"
) else (
    echo SKIP ^(exists^): prose-py-api-docs
)

if not exist "%TARGET%\microsoft-academic-services" (
    echo Cloning microsoft-academic-services...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-academic-services.git "%TARGET%\microsoft-academic-services"
) else (
    echo SKIP ^(exists^): microsoft-academic-services
)

if not exist "%TARGET%\windows-ai-docs" (
    echo Cloning windows-ai-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/windows-ai-docs.git "%TARGET%\windows-ai-docs"
) else (
    echo SKIP ^(exists^): windows-ai-docs
)

if not exist "%TARGET%\semantic-kernel-docs" (
    echo Cloning semantic-kernel-docs...
    git clone --depth 1 https://github.com/MicrosoftDocs/semantic-kernel-docs.git "%TARGET%\semantic-kernel-docs"
) else (
    echo SKIP ^(exists^): semantic-kernel-docs
)

echo.
echo Done — Group 3 complete.
