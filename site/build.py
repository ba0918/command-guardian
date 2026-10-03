#!/usr/bin/env python3
"""Build the command-guardian landing page.

The page is regenerated from the repository on every push to main:
version and release notes come from Cargo.toml and CHANGELOG.md, reference
tables and the introduction come from README.md and README-ja.md, and every
demo output is produced by running the freshly built binary.

Usage: python3 site/build.py --binary target/release/command-guardian [--out _site]
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
REPO = os.environ.get("GITHUB_REPOSITORY", "ba0918/command-guardian")
REPO_URL = f"https://github.com/{REPO}"

# Each demo runs `command-guardian check` inside a fixture project. The
# expected verdict only produces a build warning when the binary disagrees,
# so the page always shows what the current main actually does.
DEMOS = [
    {
        "command": 'D=~; rm -rf "$D"',
        "hero": True,
        "expect": "block",
        "en": "Variables are resolved without running anything: this deletes your home directory.",
        "ja": "コマンドを実行せずに変数を解決し、ホームディレクトリの削除だと見抜きます。",
    },
    {
        "command": "rm -rf .git",
        "expect": "block",
        "en": "Repository metadata is protected.",
        "ja": "リポジトリの管理情報は保護対象です。",
    },
    {
        "command": "rm -rf src",
        "hero": True,
        "expect": "ask",
        "en": "Uncommitted changes would be lost, so it asks first.",
        "ja": "コミットしていない変更が消えるため、確認を求めます。",
    },
    {
        "command": "rm -rf dist",
        "hero": True,
        "expect": "allow",
        "en": "A committed, unchanged directory in a Git worktree can be recreated.",
        "ja": "Gitで管理され、変更のないディレクトリは元に戻せます。",
    },
    {
        "command": "git push origin main",
        "expect": "ask",
        "en": "A project guard in .command-guardian.toml asks before pushing.",
        "ja": ".command-guardian.toml のguard規則で、push前に確認を求めます。",
    },
    {
        "command": "rm -rf /tmp/build-cache",
        "expect": "allow",
        "en": "Children of temporary directories are allowed.",
        "ja": "一時ディレクトリの配下は許可します。",
    },
    {
        "command": "echo done > /dev/null",
        "expect": "allow",
        "en": "Discarding output to /dev/null is not a truncation.",
        "ja": "/dev/null への出力の破棄は切り詰めとして扱いません。",
    },
    {
        "command": "dd if=/dev/zero of=/dev/sda",
        "expect": "block",
        "en": "Writes to block devices are treated as formatting.",
        "ja": "ブロックデバイスへの書き込みはフォーマットとして扱います。",
    },
]

HOOK_DEMO_COMMAND = "rm -rf .git"

PROJECT_CONFIG = """\
[[commands.guard]]
program = "git"
reason = "Review the destination and commits before pushing."
verdict = "ask"
deny = [["push"]]
"""


def fail(message: str) -> None:
    print(f"site build: {message}", file=sys.stderr)
    sys.exit(1)


def warn(message: str) -> None:
    if os.environ.get("GITHUB_ACTIONS"):
        print(f"::warning title=Landing page::{message}")
    else:
        print(f"warning: {message}", file=sys.stderr)


# ---------------------------------------------------------------- markdown


def inline(text: str) -> str:
    """Render the inline Markdown used in the README and CHANGELOG."""
    parts = re.split(r"(`[^`]+`)", text)
    out = []
    for part in parts:
        if part.startswith("`") and part.endswith("`") and len(part) > 1:
            out.append(f"<code>{html.escape(part[1:-1])}</code>")
            continue
        escaped = html.escape(part, quote=False)
        escaped = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", escaped)

        def link(match: re.Match[str]) -> str:
            label, url = match.group(1), html.unescape(match.group(2))
            if not re.match(r"[a-z]+:", url) and not url.startswith("#"):
                url = f"{REPO_URL}/blob/main/{url}"
            return f'<a href="{html.escape(url)}">{label}</a>'

        escaped = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, escaped)
        out.append(escaped)
    return "".join(out)


def sections(markdown: str) -> dict[str, list[str]]:
    """Map each heading's text to the lines below it, up to the next heading."""
    result: dict[str, list[str]] = {}
    current = ""
    result[current] = []
    in_fence = False
    for line in markdown.splitlines():
        if line.startswith("```"):
            in_fence = not in_fence
        match = None if in_fence else re.match(r"^#{1,6}\s+(.*)$", line)
        if match:
            current = match.group(1).strip()
            result[current] = []
        else:
            result[current].append(line)
    return result


def section(doc: dict[str, list[str]], heading: str, source: str) -> list[str]:
    if heading not in doc:
        fail(f'heading "{heading}" not found in {source}')
    return doc[heading]


def table(lines: list[str], source: str) -> str:
    rows = [line for line in lines if line.startswith("|")]
    if len(rows) < 3:
        fail(f"no table found in {source}")
    first = None
    for index, line in enumerate(lines):
        if line.startswith("|"):
            first = index
            break
    block = []
    for line in lines[first:]:
        if not line.startswith("|"):
            break
        block.append([cell.strip() for cell in line.strip().strip("|").split("|")])
    head, body = block[0], block[2:]
    thead = "".join(f"<th>{inline(cell)}</th>" for cell in head)
    tbody = "".join(
        "<tr>" + "".join(f"<td>{verdict_chips(inline(cell))}</td>" for cell in row) + "</tr>"
        for row in body
    )
    return f'<div class="table-wrap"><table><thead><tr>{thead}</tr></thead><tbody>{tbody}</tbody></table></div>'


def verdict_chips(cell: str) -> str:
    return re.sub(
        r"<code>(allow|ask|block)</code>",
        r'<code class="chip chip-\1">\1</code>',
        cell,
    )


def paragraphs(lines: list[str]) -> list[str]:
    result, current = [], []
    for line in lines:
        if line.strip():
            current.append(line.strip())
        elif current:
            result.append(" ".join(current))
            current = []
    if current:
        result.append(" ".join(current))
    return result


def lead(doc: dict[str, list[str]]) -> str:
    """The README introduction: the paragraphs under the top-level title."""
    title = next(key for key in doc if key)
    return "".join(f"<p>{inline(p)}</p>" for p in paragraphs(doc[title]))


# ---------------------------------------------------------------- changelog


def changelog(text: str, limit: int = 5) -> str:
    entries = []
    for match in re.finditer(
        r"^## \[([^\]]+)\](?: - (\d{4}-\d{2}-\d{2}))?\s*$(.*?)(?=^## |^\[[^\]]+\]: |\Z)",
        text,
        re.M | re.S,
    ):
        version, date, body = match.groups()
        items = [line[2:].strip() for line in body.splitlines() if line.startswith("- ")]
        # The public page lists published releases only; Unreleased has no date.
        if not items or not date:
            continue
        entries.append((version, date, items))
    if not entries:
        fail("no release found in CHANGELOG.md")
    out = []
    for version, date, items in entries[:limit]:
        tag = f'<a href="{REPO_URL}/releases/tag/v{version}">v{html.escape(version)}</a>'
        when = f'<time datetime="{date}">{date}</time>'
        lis = "".join(f"<li>{inline(item)}</li>" for item in items)
        out.append(f'<article class="release"><h3>{tag}</h3>{when}<ul>{lis}</ul></article>')
    return "".join(out)


# ---------------------------------------------------------------- demos


def fixture(base: Path) -> tuple[Path, Path]:
    """A home directory holding a Git project with one dirty directory."""
    if base.exists():
        shutil.rmtree(base)
    home = base / "home"
    project = home / "project"
    for directory in ("dist", "src"):
        (project / directory).mkdir(parents=True)
        (project / directory / "file.txt").write_text(f"{directory}\n")
    git = ["git", "-c", "user.name=demo", "-c", "user.email=demo@example.invalid"]
    for args in (["init", "-q"], ["add", "."], ["commit", "-q", "-m", "init"]):
        subprocess.run(git + args, cwd=project, check=True)
    (project / "src" / "file.txt").write_text("src\nchanged\n")
    (project / ".command-guardian.toml").write_text(PROJECT_CONFIG)
    return home, project


def run(binary: Path, home: Path, project: Path, args: list[str], stdin: str | None = None):
    env = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_STATE_HOME": str(home / ".local/state"),
        "LANG": "C.UTF-8",
    }
    proc = subprocess.run(
        [str(binary), *args],
        cwd=project,
        env=env,
        input=stdin,
        capture_output=True,
        text=True,
        timeout=30,
    )
    shown = (proc.stdout + proc.stderr).replace(str(home), "~").rstrip()
    return proc.returncode, shown


def demos(binary: Path, base: Path) -> tuple[str, str, str]:
    home, project = fixture(base)
    tabs, panels, hero = [], [], []
    for index, demo in enumerate(DEMOS):
        code, output = run(binary, home, project, ["check", demo["command"]])
        verdict = {0: "allow", 1: "ask", 2: "block"}.get(code)
        if verdict is None:
            fail(f"check {demo['command']!r} exited with {code}: {output}")
        if verdict != demo["expect"]:
            warn(f"demo {demo['command']!r} now returns {verdict}, expected {demo['expect']}")
        quoted = html.escape(repr_single(demo["command"]))
        command = html.escape(demo["command"])
        selected = "true" if index == 0 else "false"
        tabs.append(
            f'<button type="button" role="tab" id="demo-tab-{index}" aria-controls="demo-{index}" '
            f'aria-selected="{selected}" class="demo-tab demo-{verdict}">'
            f'<span class="chip chip-{verdict}">{verdict}</span><code>{command}</code></button>'
        )
        panels.append(
            f'<figure class="demo demo-{verdict}" role="tabpanel" tabindex="0" id="demo-{index}" aria-labelledby="demo-tab-{index}">'
            f'<figcaption><span class="chip chip-{verdict}">{verdict}</span>'
            f'<span class="en">{html.escape(demo["en"])}</span>'
            f'<span class="ja">{html.escape(demo["ja"])}</span></figcaption>'
            f'<pre class="term term-wrap"><span class="prompt">$</span> command-guardian check {quoted}\n'
            f"{html.escape(output)}\n"
            f'<span class="exit">exit {code}</span></pre></figure>'
        )
        if demo.get("hero"):
            first = output.splitlines()[0] if output else ""
            hero.append((
                ["allow", "ask", "block"].index(verdict),
                f'<span class="prompt">$</span> command-guardian check {quoted}\n'
                f'<span class="v-{verdict}">{html.escape(first)}</span>',
            ))
    explorer = (
        '<div class="demo-explorer">'
        f'<div class="demo-list" role="tablist" aria-label="Examples" data-aria-en="Examples" data-aria-ja="判定例">{"".join(tabs)}</div>'
        f'<div class="demo-panels">{"".join(panels)}</div></div>'
    )
    hero_term = '<pre class="term term-wrap hero-term">' + "\n\n".join(text for _, text in sorted(hero)) + "</pre>"

    request = {
        "tool_name": "Bash",
        "tool_input": {"command": HOOK_DEMO_COMMAND},
        "cwd": str(project),
    }
    code, output = run(binary, home, project, ["hook", "--agent", "claude"], json.dumps(request))
    if code != 0 or not output:
        fail(f"hook demo exited with {code} and output {output!r}")
    try:
        output = json.dumps(json.loads(output), indent=2, ensure_ascii=False)
    except json.JSONDecodeError:
        fail(f"hook demo did not print JSON: {output!r}")
    shown_request = json.dumps({**request, "cwd": "~/project"})
    hook = (
        f'<pre class="term term-wrap"><span class="prompt">$</span> echo {html.escape(repr_single(shown_request))} |\n'
        f"    command-guardian hook --agent claude\n{html.escape(output)}</pre>"
    )
    return explorer, hook, hero_term


def repr_single(text: str) -> str:
    return "'" + text.replace("'", "'\\''") + "'"


# ---------------------------------------------------------------- page


def git_commit() -> str:
    sha = os.environ.get("GITHUB_SHA")
    if sha:
        return sha
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def build(binary: Path, out: Path) -> None:
    if not binary.is_file():
        fail(f"binary not found: {binary} (run cargo build --release --locked first)")
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    version = cargo["package"]["version"]
    readme_en = sections((ROOT / "README.md").read_text())
    readme_ja = sections((ROOT / "README-ja.md").read_text())
    demo_cards, hook_demo, hero_demo = demos(binary, ROOT / "target" / "site-demo")
    sha = git_commit()

    values = {
        "version": html.escape(version),
        "repo_url": REPO_URL,
        "commit": html.escape(sha),
        "commit_short": html.escape(sha[:7]),
        "lead_en": lead(readme_en),
        "lead_ja": lead(readme_ja),
        "demos": demo_cards,
        "hook_demo": hook_demo,
        "hero_demo": hero_demo,
        "targets_en": table(section(readme_en, "How targets affect the verdict", "README.md"), "README.md"),
        "targets_ja": table(section(readme_ja, "対象パスと判定の関係", "README-ja.md"), "README-ja.md"),
        "exit_en": table(section(readme_en, "Check a command", "README.md"), "README.md"),
        "exit_ja": table(section(readme_ja, "コマンドの判定", "README-ja.md"), "README-ja.md"),
        "hooks_en": table(section(readme_en, "Agent hooks", "README.md"), "README.md"),
        "hooks_ja": table(section(readme_ja, "エージェントのフック", "README-ja.md"), "README-ja.md"),
        "settings_en": table(section(readme_en, "Configuration", "README.md"), "README.md"),
        "settings_ja": table(section(readme_ja, "設定", "README-ja.md"), "README-ja.md"),
        "changelog": changelog((ROOT / "CHANGELOG.md").read_text()),
    }

    page = (SITE / "template.html").read_text()

    def substitute(match: re.Match[str]) -> str:
        key = match.group(1)
        if key not in values:
            fail(f"unknown placeholder {{{{{key}}}}} in template.html")
        return values[key]

    page = re.sub(r"\{\{(\w+)\}\}", substitute, page)

    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    (out / "index.html").write_text(page)
    shutil.copy(SITE / "style.css", out / "style.css")
    shutil.copy(SITE / "favicon.svg", out / "favicon.svg")
    (out / ".nojekyll").write_text("")
    print(f"site build: wrote {out / 'index.html'} for v{version} at {sha[:7]}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/command-guardian")
    parser.add_argument("--out", type=Path, default=ROOT / "_site")
    args = parser.parse_args()
    build(args.binary.resolve(), args.out.resolve())


if __name__ == "__main__":
    main()
