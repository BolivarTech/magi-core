#!/usr/bin/env python3
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-09-13
"""One-shot migration: OpenAiCompatibleProvider::new / ::with_timeout -> ::with_dialect.

This script is one-shot and is NOT wired into CI. It is kept in the tree only so that
its diff remains auditable. The default mode is a dry run: it prints what it would do
and writes nothing.
"""
from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

ROOTS = ("src", "tests", "examples", "smoke/src")
EXPECTED_SITES = 29
EXCLUDED_MULTILINE = (
    ("src/providers/openai_compat.rs", 274),
    ("src/providers/openai_compat.rs", 714),
    ("examples/basic_analysis.rs", 164),
)
OLD_NEW = "OpenAiCompatibleProvider::new("
OLD_WITH_TIMEOUT = "OpenAiCompatibleProvider::with_timeout("
NEW_CTOR = "OpenAiCompatibleProvider::with_dialect("
EXIT_OK = 0
EXIT_COUNT_MISMATCH = 1
EXIT_USAGE = 2


@dataclass(frozen=True)
class Site:
    """A discovered call site that may need migration.

    Attributes:
        path: Relative path from the repo root, using POSIX separators.
        line_no: One-based line number of the matching line.
        text: The original line without its trailing newline.
        excluded: Whether this site is a known multiline call to be skipped.
    """
    path: str
    line_no: int
    text: str
    excluded: bool


def discover(repo_root: Path) -> list[Site]:
    """Find every line under ROOTS that calls the old constructors.

    Only ``*.rs`` files under the configured ROOTS entries are scanned, in sorted
    path order. A site is recorded for each line containing one of the old
    constructor names, regardless of whether the call is single- or multi-line.

    Args:
        repo_root: Absolute or relative path to the repository root.

    Returns:
        A sorted list of discovered sites.

    Raises:
        FileNotFoundError: If a configured root does not exist under repo_root.
    """
    sites: list[Site] = []
    for root_name in ROOTS:
        root = repo_root / root_name
        if not root.exists():
            raise FileNotFoundError(f"Missing root: {root}")
        for path in sorted(root.rglob("*.rs")):
            rel = path.relative_to(repo_root).as_posix()
            with path.open("r", encoding="utf-8") as f:
                for line_no, raw in enumerate(f, start=1):
                    text = raw.rstrip("\r\n")
                    if OLD_NEW in text or OLD_WITH_TIMEOUT in text:
                        excluded = (rel, line_no) in EXCLUDED_MULTILINE
                        sites.append(Site(rel, line_no, text, excluded))
    return sites


def qualified_paths(path: str) -> tuple[str, str]:
    """Return the dialect and timeout expressions for a given file path.

    The expressions are fully qualified so that no extra ``use`` statements are
    needed; duplicate or unused imports would break builds with ``-D warnings``.

    Args:
        path: Repository-relative path to the file containing the call site.

    Returns:
        A tuple of ``(dialect_expr, timeout_expr)``.

    Raises:
        ValueError: If the path does not match any known prefix pattern.
    """
    if path == "src/providers/openai_compat.rs":
        return (
            "Dialect::MaxTokens",
            "crate::provider::DEFAULT_CLIENT_TIMEOUT",
        )
    if path.startswith("src/"):
        return (
            "crate::providers::openai_compat::Dialect::MaxTokens",
            "crate::provider::DEFAULT_CLIENT_TIMEOUT",
        )
    if path.startswith(("tests/", "examples/", "smoke/src/")):
        return (
            "magi_core::providers::openai_compat::Dialect::MaxTokens",
            "magi_core::provider::DEFAULT_CLIENT_TIMEOUT",
        )
    raise ValueError(f"No qualified-path mapping for: {path!r}")


def _find_matching_close_paren(text: str, open_index: int) -> int:
    """Return the index of the matching ``)`` starting from ``open_index``.

    Args:
        text: The line containing an opening parenthesis.
        open_index: Index in ``text`` of the opening ``(``.

    Returns:
        Index of the matching closing parenthesis.

    Raises:
        ValueError: If no matching closing parenthesis exists on the line.
    """
    depth = 1
    for i in range(open_index + 1, len(text)):
        ch = text[i]
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return i
    raise ValueError("matching ')' not found on this line (multiline call?)")


def _split_last_argument(args: str) -> tuple[str, str]:
    """Split an argument list so the dialect can be inserted before the last arg.

    Args:
        args: Comma-separated argument list, with no surrounding parentheses.

    Returns:
        A tuple ``(head, last)`` where ``head`` is everything before the last
        top-level comma and ``last`` is the final argument.

    Raises:
        ValueError: If no top-level comma exists in the argument list.
    """
    depth = 0
    last_comma = -1
    for i, ch in enumerate(args):
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        elif ch == "," and depth == 0:
            last_comma = i
    if last_comma == -1:
        raise ValueError("no top-level comma separating timeout argument")
    return (args[:last_comma].rstrip(), args[last_comma + 1:].lstrip())


def rewrite_line(text: str, path: str) -> str:
    """Rewrite a single-line old constructor call to ``::with_dialect``.

    The constructor name and its matching closing parenthesis are located by
    counting parentheses depth. Nested calls such as ``format!(...)`` and
    ``Some(...into())`` are handled correctly because string literals in these
    lines never contain parentheses. Leading whitespace, prefixes, suffixes
    such as ``.unwrap()`` or ``.expect(...)``, and trailing ``;`` are preserved.

    Args:
        text: The original line text without its newline.
        path: Repository-relative path to the file containing the line.

    Returns:
        The rewritten line.

    Raises:
        ValueError: If the call has no matching closing parenthesis on the line,
            or if the constructor name is not recognized.
    """
    dialect_expr, timeout_expr = qualified_paths(path)
    if OLD_NEW in text:
        old_ctor = OLD_NEW[:-1]
        is_new = True
    elif OLD_WITH_TIMEOUT in text:
        old_ctor = OLD_WITH_TIMEOUT[:-1]
        is_new = False
    else:
        raise ValueError("line does not contain an old constructor call")
    new_ctor = NEW_CTOR[:-1]

    ctor_start = text.index(old_ctor)
    open_paren = ctor_start + len(old_ctor)
    close_paren = _find_matching_close_paren(text, open_paren)

    args = text[open_paren + 1:close_paren]
    if is_new:
        new_args = f"{args}, {dialect_expr}, {timeout_expr}"
    else:
        head, last = _split_last_argument(args)
        new_args = f"{head}, {dialect_expr}, {last}"

    return text[:ctor_start] + new_ctor + "(" + new_args + text[close_paren:]


def main() -> None:
    """Parse arguments and run the migration in dry-run or apply mode.

    Raises:
        SystemExit: Always; EXIT_OK on success, EXIT_COUNT_MISMATCH when the
            count guard or a rewrite error occurs, EXIT_USAGE for bad invocation.
    """
    parser = argparse.ArgumentParser(
        description="Migrate old OpenAI-compatible constructors to ::with_dialect.",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Apply rewrites; default is a dry run that writes nothing.",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="Repository root (default: parent of the directory containing this script).",
    )
    args = parser.parse_args()
    repo_root = args.repo_root.resolve()

    try:
        sites = discover(repo_root)
    except FileNotFoundError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        sys.exit(EXIT_COUNT_MISMATCH)

    rewrites: list[tuple[Site, str]] = []
    errors: list[tuple[Site, str]] = []
    for site in sites:
        if site.excluded:
            print(f"SKIP {site.path}:{site.line_no}  (multiline, migrate by hand)")
            continue
        try:
            rewritten = rewrite_line(site.text, site.path)
        except ValueError as exc:
            errors.append((site, str(exc)))
            print(f"ERROR {site.path}:{site.line_no} {exc}")
            continue
        print(f"DRY  {site.path}:{site.line_no}  {site.text.strip()}")
        print(f"  -> {rewritten.strip()}")
        rewrites.append((site, rewritten))

    if errors:
        print("ERRORS during rewrite; nothing written", file=sys.stderr)
        sys.exit(EXIT_COUNT_MISMATCH)

    count = len(rewrites)
    if count != EXPECTED_SITES:
        print(
            f"COUNT MISMATCH: found {count}, expected {EXPECTED_SITES} -- nothing written",
            file=sys.stderr,
        )
        sys.exit(EXIT_COUNT_MISMATCH)

    if not args.apply:
        print(
            f"dry run: {count} sites would change, 3 excluded; re-run with --apply"
        )
        sys.exit(EXIT_OK)

    changes_by_file: dict[str, dict[int, str]] = {}
    for site, rewritten in rewrites:
        changes_by_file.setdefault(site.path, {})[site.line_no] = rewritten

    for rel, line_changes in changes_by_file.items():
        file_path = repo_root.joinpath(*rel.split("/"))
        with file_path.open("r", encoding="utf-8", newline="") as f:
            lines = f.readlines()
        for line_no, new_text in line_changes.items():
            old_line = lines[line_no - 1]
            ending = old_line[len(old_line.rstrip("\r\n")):]
            lines[line_no - 1] = new_text + ending
        with file_path.open("w", encoding="utf-8", newline="") as f:
            f.writelines(lines)

    print(f"applied: {count} sites in {len(changes_by_file)} files")
    sys.exit(EXIT_OK)


if __name__ == "__main__":
    main()
