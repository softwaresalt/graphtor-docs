@echo off
REM ======================================================================
REM Group 37: COMMUNITY & LEARNING
REM Developer advocacy content, community blogs, labs, and open learning resources — use together for outreach, self-study, or evangelism content.
REM ======================================================================

SET BASE_PATH=E:\Source\ms-docs
SET TARGET=%BASE_PATH%\community-and-learning

echo.
echo ======================================================================
echo  Group 37: COMMUNITY & LEARNING
echo  Target: %TARGET%
echo  Repos:  7
echo ======================================================================
echo.

if not exist "%TARGET%" mkdir "%TARGET%"

if not exist "%TARGET%\cloud-developer-advocates" (
    echo Cloning cloud-developer-advocates...
    git clone --depth 1 https://github.com/MicrosoftDocs/cloud-developer-advocates.git "%TARGET%\cloud-developer-advocates"
) else (
    echo SKIP ^(exists^): cloud-developer-advocates
)

if not exist "%TARGET%\microsoft-365-community" (
    echo Cloning microsoft-365-community...
    git clone --depth 1 https://github.com/MicrosoftDocs/microsoft-365-community.git "%TARGET%\microsoft-365-community"
) else (
    echo SKIP ^(exists^): microsoft-365-community
)

if not exist "%TARGET%\community-content" (
    echo Cloning community-content...
    git clone --depth 1 https://github.com/MicrosoftDocs/community-content.git "%TARGET%\community-content"
) else (
    echo SKIP ^(exists^): community-content
)

if not exist "%TARGET%\open_specs_blog" (
    echo Cloning open_specs_blog...
    git clone --depth 1 https://github.com/MicrosoftDocs/open_specs_blog.git "%TARGET%\open_specs_blog"
) else (
    echo SKIP ^(exists^): open_specs_blog
)

if not exist "%TARGET%\DevOpsLearn" (
    echo Cloning DevOpsLearn...
    git clone --depth 1 https://github.com/MicrosoftDocs/DevOpsLearn.git "%TARGET%\DevOpsLearn"
) else (
    echo SKIP ^(exists^): DevOpsLearn
)

if not exist "%TARGET%\FastTrackBlogRepo" (
    echo Cloning FastTrackBlogRepo...
    git clone --depth 1 https://github.com/MicrosoftDocs/FastTrackBlogRepo.git "%TARGET%\FastTrackBlogRepo"
) else (
    echo SKIP ^(exists^): FastTrackBlogRepo
)

if not exist "%TARGET%\learn" (
    echo Cloning learn...
    git clone --depth 1 https://github.com/MicrosoftDocs/learn.git "%TARGET%\learn"
) else (
    echo SKIP ^(exists^): learn
)

echo.
echo Done — Group 37 complete.
