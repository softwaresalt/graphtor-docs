"""Generate per-group batch scripts from ms-docs-grouped.txt."""

import os
import re
import textwrap

GROUPED_FILE = os.path.join(os.path.dirname(__file__), "..", "paths", "ms-docs-grouped.txt")
OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "..", ".scripts", "clone-groups")


def sanitize_folder_name(title: str) -> str:
    """Turn a group title like 'AZURE CORE & CLI' into 'azure-core-and-cli'.

    Special cases handled:
    - ``C++`` -> ``cpp`` (avoids ``candand`` artefact from naive ``+``->``and`` replacement)
    - ``&`` -> ``and``
    - Repeated ``-and-and`` sequences collapsed to ``-and``
    """
    name = title.lower()
    # Must special-case C++ before generic '+' replacement.
    name = re.sub(r"c\+\+", "cpp", name)
    name = name.replace("&", "and")
    name = re.sub(r"[^a-z0-9]+", "-", name)
    # Collapse repeated "and" tokens that result from e.g. "& LANGUAGE" -> "and-language"
    # alongside an earlier "and" already in the title: "net-and-and-lang" -> "net-and-lang".
    name = re.sub(r"(?:-and)+(-and)", r"\1", name)
    return name.strip("-")


def parse_groups(path):
    groups = []
    current_number = None
    current_title = None
    current_description = None
    current_urls = []
    desc_lines = []

    with open(path, encoding="utf-8") as f:
        for raw_line in f:
            line = raw_line.rstrip("\n")

            # Detect group header: "# GROUP N: TITLE"
            m = re.match(r"^#\s+GROUP\s+(\d+):\s+(.+)$", line)
            if m:
                # Save previous group
                if current_number is not None:
                    groups.append((current_number, current_title, current_description, current_urls))
                current_number = int(m.group(1))
                current_title = m.group(2).strip()
                current_urls = []
                desc_lines = []
                current_description = ""
                continue

            # Collect description lines (comment lines between header and first URL)
            if current_number is not None and not current_urls and line.startswith("# ") and not line.startswith("# =="):
                desc_lines.append(line[2:].strip())
                current_description = " ".join(desc_lines)
                continue

            # Collect URLs
            if line.startswith("https://github.com/"):
                if current_number is not None:
                    current_urls.append(line.strip())

    # Don't forget last group
    if current_number is not None:
        groups.append((current_number, current_title, current_description, current_urls))

    return groups


def generate_batch(number, title, description, urls, output_dir):
    folder = sanitize_folder_name(title)
    padded = f"{number:02d}"
    filename = f"clone-{padded}-{folder}.bat"
    filepath = os.path.join(output_dir, filename)

    clone_lines = []
    for url in urls:
        # Extract repo name from clone URL (strip trailing .git)
        repo_name = url.rsplit("/", 1)[-1]
        if repo_name.endswith(".git"):
            repo_name = repo_name[:-4]
        clone_lines.append(
            f'if not exist "%TARGET%\\{repo_name}" (\n'
            f'    echo Cloning {repo_name}...\n'
            f'    git clone --depth 1 {url} "%TARGET%\\{repo_name}"\n'
            f') else (\n'
            f'    echo SKIP ^(exists^): {repo_name}\n'
            f')'
        )

    content = textwrap.dedent(f"""\
        @echo off
        REM ======================================================================
        REM Group {number}: {title}
        REM {description}
        REM ======================================================================

        SET BASE_PATH=E:\\Source\\ms-docs
        SET TARGET=%BASE_PATH%\\{folder}

        echo.
        echo ======================================================================
        echo  Group {number}: {title}
        echo  Target: %TARGET%
        echo  Repos:  {len(urls)}
        echo ======================================================================
        echo.

        if not exist "%TARGET%" mkdir "%TARGET%"

    """)

    content += "\n\n".join(clone_lines)
    content += "\n\necho.\necho Done — Group " + str(number) + " complete.\n"

    with open(filepath, "w", encoding="utf-8", newline="\r\n") as f:
        f.write(content)

    return filename


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    groups = parse_groups(GROUPED_FILE)
    print(f"Parsed {len(groups)} groups from grouped file.\n")

    for number, title, description, urls in groups:
        fname = generate_batch(number, title, description, urls, OUTPUT_DIR)
        print(f"  [{number:2d}] {fname}  ({len(urls)} repos)")

    # Also generate a run-all script
    run_all_path = os.path.join(OUTPUT_DIR, "clone-all.bat")
    with open(run_all_path, "w", encoding="utf-8", newline="\r\n") as f:
        f.write("@echo off\n")
        f.write("REM Run all group clone scripts sequentially.\n")
        f.write("REM Edit this file to comment out groups you don't need.\n\n")
        for number, title, _, urls in groups:
            folder = sanitize_folder_name(title)
            padded = f"{number:02d}"
            bat = f"clone-{padded}-{folder}.bat"
            f.write(f'call "%~dp0{bat}"\n')
        f.write("\necho.\necho All groups complete.\n")

    print(f"\n  Created clone-all.bat (runs all groups)")
    print(f"\nAll scripts written to: {OUTPUT_DIR}")


if __name__ == "__main__":
    main()
