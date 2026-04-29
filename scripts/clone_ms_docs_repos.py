"""
Fetch GitHub org repo clone URLs and write them to a file in ./paths/.

Usage:
    python clone_ms_docs_repos.py https://api.github.com/orgs/MicrosoftDocs ms-docs.txt

Set GITHUB_TOKEN env var for higher API rate limits.
"""

import argparse
import os
import sys

import requests


def get_all_repos(base_url, github_token=None):
    headers = {"Accept": "application/vnd.github.v3+json"}
    if github_token:
        headers["Authorization"] = f"token {github_token}"

    repos = []
    page = 1

    while True:
        url = f"{base_url}/repos?per_page=100&page={page}"
        response = requests.get(url, headers=headers, timeout=30)
        response.raise_for_status()

        data = response.json()
        if not data:
            break

        for repo in data:
            # Filter out localization repos (contain a dot but don't end in .en-us)
            if not repo["name"].count(".") > 0 or repo["name"].endswith(".en-us"):
                repos.append(repo["clone_url"])
        page += 1

    return repos


def main():
    parser = argparse.ArgumentParser(description="Fetch GitHub org repo clone URLs and write them to a file.")
    parser.add_argument("base_url", help="GitHub API base URL for the org (e.g. https://api.github.com/orgs/MicrosoftDocs)")
    parser.add_argument("output_file", help="Output filename (written to ./paths/, e.g. ms-docs.txt)")
    args = parser.parse_args()

    token = os.environ.get("GITHUB_TOKEN")
    if not token:
        print("Tip: Set GITHUB_TOKEN env var to avoid API rate limits.", file=sys.stderr)

    print(f"Fetching repo list from {args.base_url}...")
    urls = get_all_repos(args.base_url, github_token=token)
    print(f"Discovered {len(urls)} repositories.")

    paths_dir = os.path.join(os.path.dirname(__file__), "..", "paths")
    os.makedirs(paths_dir, exist_ok=True)
    output_path = os.path.join(paths_dir, args.output_file)

    with open(output_path, "w", encoding="utf-8") as f:
        for url in urls:
            f.write(url + "\n")

    print(f"Wrote {len(urls)} clone URLs to {output_path}")


if __name__ == "__main__":
    main()
