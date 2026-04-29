"""
Comment out non-documentation repos in ms-docs-grouped.txt.

Targets: SDK source code, code samples, VS solutions, Jupyter notebooks,
pipeline sample repos, generated API references, tools/utilities, REST API
specs (JSON/YAML), and mslearn-* exercise/lab repos.
"""

import os
import re

GROUPED_FILE = os.path.join(os.path.dirname(__file__), "..", "paths", "ms-docs-grouped.txt")

# ── Exact repo names (suffix after MicrosoftDocs/) to comment out ────────────

COMMENT_OUT = {
    # Group 1 — REST specs (JSON/YAML) and sample config files
    "azure-rest-api-specs",
    "azure-cloud-services-files",

    # Group 3 — ML framework code and Jupyter notebook repos
    "CNTK",
    "Azure-MachineLearning-DataScience",
    "tensorflow",
    "tensorflowfundamentals",
    "pytorchfundamentals",
    "ml-basics",
    "ai-fundamentals",
    "tensorflow-fundamentals",

    # Group 4 — Generated SDK ref and library code
    "hpc-docs-sdk-dotnet",
    "Microsoft-MPI",

    # Group 5 — Code samples and REST specs
    "dataexplorer-docs-samples",
    "azure-cosmos-db-graph-dotnet-getting-started",
    "azure-digitaltwins-rest-api",

    # Group 8 — SDK code, tool code, REST specs
    "mipsdk-dotnet",
    "WDAC-Toolkit",
    "projectfreta-docs-rest-apis",

    # Group 9 — MSAL library source code (not docs about them)
    "microsoft-authentication-library-javascript",
    "microsoft-authentication-library-for-python",
    "microsoft-authentication-library-java",
    "microsoft-authentication-library-dotnet",
    "microsoft-authentication-library-objc",
    "microsoft-authentication-library-for-go",

    # Group 10 — Pipeline sample repos and YAML config samples
    "azure-pipelines-canary-k8s",
    "codecoverage-yaml-samples",
    "pipelines-dotnet-core",
    "pipelines-dotnet-core-docker",
    "pipelines-javascript",
    "pipelines-javascript-docker",
    "pipelines-java",
    "pipelines-java-function",
    "pipelines-go",
    "pipelines-go-docker",
    "pipelines-python-django",
    "pipelines-php",
    "pipelines-ruby",
    "pipelines-xamarin",
    "pipelines-xcode",
    "pipelines-android",
    "pipelines-anaconda",
    "pipelines-azureml",
    "pipelines-cpp",
    "pipelines-vmss",
    "pipelines-multistage",

    # Group 11 — Query examples and REST specs
    "LogAnalyticsExamples",
    "opsmgr-docs-rest-apis",

    # Group 12 — SDK code and learning path code samples
    "azure-iot-sdk-csharp",
    "azure-iot-docs-sdk-typescript",
    "Azure-Sphere-Developer-Learning-Path",

    # Group 13 — PowerShell tools
    "AzureStack-Tools",

    # Group 14 — Generated PowerShell cmdlet reference
    "sccm-docs-powershell-ref",

    # Group 16 — Generated .NET API references (PIA/SDK refs), SDK code, DSC module
    "office-developer-exchange-ews-proxy-ref-dotnet",
    "office-developer-exchange-ews-managed-api-ref-dotnet",
    "office-developer-lync-client-ref-dotnet",
    "office-developer-lync-wfp-ref-dotnet",
    "office-developer-lync-persistent-chat-ref-dotnet",
    "office-developer-managed-sip-api-ref-dotnet",
    "office-developer-ucma-api-ref-dotnet",
    "office-developer-ucma-voice-ref-dotnet",
    "office-developer-o365-service-communications-ref-dotnet",
    "office-developer-infopath-external-automation-ref-dotnet",
    "office-developer-infopath-form-templates-ref-dotnet",
    "office-developer-project-class-web-ref-dotnet",
    "office-developer-excel-pia-ref-dotnet",
    "office-developer-office-pia-ref-dotnet",
    "office-developer-outlook-pia-ref-dotnet",
    "office-developer-word-pia-ref-dotnet",
    "office-developer-sharepoint-server-2013-ref-dotnet",
    "groove-api-sdk-csharp",
    "Office365DSC",

    # Group 17 — Generated SDK types and PowerShell tools
    "msteam-docs-sdk-typescript",
    "Teams-Auto-Attendant-and-Call-Queue-Backup-and-Bulk-Provisioning-Tools",

    # Group 20 — Bot Framework SDK source and generated SDK references
    "botbuilder-dotnet",
    "botbuilder-docs-sdk-typescript",
    "botbuilder-docs-sdk-python",

    # Group 21 — API code
    "XboxLive-API",

    # Group 22 — Code samples, generated API refs, sample apps
    "windows-topic-specific-samples",
    "community-toolkit-api-ref-dotnet",
    "webview2-win32-reference",
    "webview2-winrt-reference",
    "SimpleRecorder",

    # Group 24 — Code samples
    "powershell-sdk-samples",

    # Group 25 — Code samples
    "vs-tutorial-samples",

    # Group 26 — Assets, demos, and ramp-up code
    "dotnet-iot-assets",
    "dotnet-ci-demo",
    "devrampup",

    # Group 29 — REST API specs
    "powerapps-docs-rest-apis",

    # Group 31 — Code examples, JS SDK code, mockups
    "python-sdk-docs-examples",
    "azure-cognitive-services-js",
    "azure-fluid-preview-pr",
    "azure-sdk-docs-js-mockup",

    # Group 37 — Lab exercise code
    "labs",

    # Group 39 non-mslearn — Tutorial/exercise code
    "ef-core-for-beginners",
    "minimal-api-work-with-databases",
    "vue-docs-image-recognition",
    "Train-package-Azure-ML-module-for-IoT-Edge",
}

# ── Prefix patterns that are always exercise/lab code (not docs) ─────────────

COMMENT_OUT_PREFIXES = (
    "mslearn-",
    "ms-learn-",
    "MSLearn-",
    "mslearn_",
)


def repo_name_from_url(url):
    """Extract repo name from a clone URL."""
    name = url.strip().rsplit("/", 1)[-1]
    if name.endswith(".git"):
        name = name[:-4]
    return name


def should_comment_out(url):
    name = repo_name_from_url(url)
    if name in COMMENT_OUT:
        return True
    if any(name.startswith(p) for p in COMMENT_OUT_PREFIXES):
        return True
    return False


def main():
    with open(GROUPED_FILE, encoding="utf-8") as f:
        lines = f.readlines()

    commented = 0
    kept = 0
    output = []

    for line in lines:
        stripped = line.rstrip("\n")
        if stripped.startswith("https://github.com/MicrosoftDocs/"):
            if should_comment_out(stripped):
                # Comment it out with a reason hint
                name = repo_name_from_url(stripped)
                output.append(f"# {stripped}  # non-doc: code/samples/tools/generated\n")
                commented += 1
            else:
                output.append(line)
                kept += 1
        else:
            output.append(line)

    with open(GROUPED_FILE, "w", encoding="utf-8") as f:
        f.writelines(output)

    print(f"Commented out: {commented} repos")
    print(f"Kept:          {kept} repos")
    print(f"Total URLs:    {commented + kept}")


if __name__ == "__main__":
    main()
