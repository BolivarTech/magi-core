#!/usr/bin/env python3
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-09-05
#
# Every source file this release touched must carry the release's version in its
# three-line header.
#
# The rule was stated in the plan and enforced by nobody, and it HAD drifted:
# `src/consensus.rs` and `src/verdict_markers.rs` both said `1.0.0` while both were
# modified on 2026-07-30, in `3.0.2`. Those were false statements about the file
# that contained them -- the same class this release corrects in rustdoc, one level
# up in the header. Both read `4.1.0` since the repair in this milestone; the past
# tense is deliberate, because a present-tense claim here would be the same defect.
#
# RELEASE PATH, never the round gate. During a cycle the headers are legitimately
# out of date: the rule syncs them WHEN A MILESTONE CLOSES, so in the round gate
# this would fire on every commit. It runs AFTER the version bump -- run before,
# it compares against the OLD version and blesses exactly the files it exists to
# catch.
#
# WHAT IS CHECKED IS `Version`, NOT `Date`, and the asymmetry is declared rather
# than left to be noticed: `Version` has ONE correct value -- the crate's -- while
# `Date` has one per file, so checking it mechanically means asking git for each
# file's last change and deciding an acceptable margin. A stale date is cosmetic;
# a stale version is not.
#
# IT DETECTS A STALE HEADER, NOT AN ABSENT ONE. The set is "files that already
# carry the three-line header", so a new file that never had one is passed over.
# Review covers that; it is declared debt, not an oversight.
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# The roots that hold versioned sources. `ci/*.sh` is deliberately NOT here: those
# headers are exempted NORMATIVELY by the contract, not because they float free of
# the crate -- an earlier version of this comment claimed the latter and this very
# milestone falsifies it, having bumped four of them to 4.1.0 alongside the crate.
# The consequence is worth knowing rather than hiding: those four are outside every
# mechanical check here, so their headers are kept by review alone.
ROOTS = ("src", "tests", "examples", "benches", "smoke/src")

# Files whose header was stale from BEFORE the last tag. Without the pin they would
# not appear in "modified since the last tag" at all, and the check would have been
# blind to exactly the drift it exists to repair.
#
# The pin is CONDITIONAL and it has ALREADY switched itself off -- earlier than the
# rationale first written here predicted. That text said the two would re-enter
# "once the repair ships and is tagged"; in fact the repair commit modified them,
# so they are reached by the GENERAL RULE and the pin no longer does any work --
# no tag involved. Stated in that order because the version comparison is not
# what stopped it: `if pinned in candidates: continue` fires first, so
# `(4,1,0) < (4,0,0)` is never evaluated for these two at all. The mechanism was
# right; the story about when and why it would stop was not.
PINNED = ("src/consensus.rs", "src/verdict_markers.rs")

HEADER_VERSION = re.compile(r"^//\s*Version:\s*(\S+)\s*$", re.M)
MANIFEST_VERSION = re.compile(r'^version\s*=\s*"([^"]+)"', re.M)
TAG = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")


def parse_version(text):
    """Return ``(major, minor, patch)`` as ints, or ``None`` when unparseable.

    Three integer fields compared NUMERICALLY, never as text -- string order puts
    `3.10.0` below `3.9.0`. No `semver` crate or package is used: it is not in the
    tree and this release adds no dependency.
    """
    parts = (text or "").strip().split(".")
    if len(parts) != 3:
        return None
    out = []
    for part in parts:
        if not part.isdigit():
            return None
        out.append(int(part))
    return tuple(out)


def git(args, cwd=None):
    return subprocess.run(["git"] + args, cwd=cwd, capture_output=True,
                          text=True, check=False)


def last_tag(cwd=None):
    """The HIGHEST `vX.Y.Z` tag by version, not the most recent by date.

    A tag that was deleted and re-created changes date order but not version
    order, and the question this answers is "which release is the tree at".
    """
    out = git(["tag", "--list", "v*"], cwd=cwd)
    best = None
    for line in out.stdout.splitlines():
        m = TAG.match(line.strip())
        if not m:
            continue
        version = tuple(int(g) for g in m.groups())
        if best is None or version > best[0]:
            best = (version, line.strip())
    return best


def changed_since(tag, cwd=None):
    """Paths modified since ``tag``, with deletions filtered out.

    `--diff-filter=d` drops the DELETED side; `-M` makes git report a rename as a
    single destination path under `--name-only`, which is the path whose header
    matters. Below `-M`'s similarity threshold git reports delete + add instead,
    and the added side survives the same filter -- both branches land on the file
    that exists, so the set does not depend on that heuristic.

    Returns None when git FAILS, and the caller treats that as a hard failure.
    Reading only stdout meant a `git diff` that errored -- a tag ref whose
    object is missing, a corrupt index -- produced no candidates, no findings,
    and `OK: every checked header matches`, over a header that was stale. That
    is the guard approving by starvation, which this module says it refuses; it
    refused it for `last_tag` and not here.
    """
    out = git(["diff", "--name-only", "--diff-filter=d", "-M",
               "%s..HEAD" % tag], cwd=cwd)
    if out.returncode != 0:
        return None
    return [line.strip() for line in out.stdout.splitlines() if line.strip()]


def in_scope(path_str, root_dir):
    if not path_str.endswith(".rs"):
        return False
    if not any(path_str == r or path_str.startswith(r + "/") for r in ROOTS):
        return False
    return (root_dir / path_str).is_file()


def check(root_dir=Path("."), manifest=None):
    """Run the guard. Returns ``(exit_code, lines)``."""
    lines = []
    manifest = manifest or (root_dir / "Cargo.toml")
    text = manifest.read_text(encoding="utf-8") if manifest.is_file() else ""
    m = MANIFEST_VERSION.search(text)
    want = parse_version(m.group(1)) if m else None
    if want is None:
        return 1, ["FAIL: no parseable `version` in %s" % manifest.as_posix()]

    tag = last_tag(root_dir)
    if tag is None:
        # FAIL CLOSED. A shallow CI checkout carries no tags, and answering "no
        # findings" there would be a guard approving by starvation.
        return 1, ["FAIL: no reachable vX.Y.Z tag -- cannot tell which release "
                   "the tree is at. A shallow checkout without tags is the "
                   "usual cause; fetch them."]
    tag_version, tag_name = tag

    # An absent root is SAID, never silently treated as "no findings".
    for r in ROOTS:
        if not (root_dir / r).is_dir():
            lines.append("SKIP: root %s does not exist" % r)

    changed = changed_since(tag_name, root_dir)
    if changed is None:
        # FAIL CLOSED, same reason as the missing tag above: a git call that
        # errored tells us nothing about the headers, and "no findings" would
        # be a guard reporting on work it could not do.
        return 1, ["FAIL: `git diff %s..HEAD` failed -- cannot tell which "
                   "files changed since the last release." % tag_name]
    candidates = [p for p in changed if in_scope(p, root_dir)]

    for pinned in PINNED:
        if pinned in candidates or not (root_dir / pinned).is_file():
            continue
        header = HEADER_VERSION.search(
            (root_dir / pinned).read_text(encoding="utf-8"))
        got = parse_version(header.group(1)) if header else None
        # Only while it is BELOW the last tag. Once repaired and tagged the
        # condition is false and the pin stops applying, on its own.
        if got is None or got < tag_version:
            candidates.append(pinned)

    findings = []
    for path_str in sorted(set(candidates)):
        body = (root_dir / path_str).read_text(encoding="utf-8", errors="replace")
        header = HEADER_VERSION.search(body)
        if header is None:
            # Not in the set: the rule governs files that ALREADY carry the
            # header. Saying so keeps the count honest.
            continue
        got = parse_version(header.group(1))
        if got is None:
            findings.append("%s: header version %r does not parse as X.Y.Z"
                            % (path_str, header.group(1)))
        elif got != want:
            findings.append("%s: header says %d.%d.%d, Cargo.toml says %d.%d.%d"
                            % ((path_str,) + got + want))

    if findings:
        lines.append("FAIL: %d header(s) out of sync with Cargo.toml (last tag %s)"
                     % (len(findings), tag_name))
        lines.extend("  " + f for f in findings)
        return 1, lines
    lines.append("OK: every checked header matches %d.%d.%d (last tag %s)"
                 % (want + (tag_name,)))
    return 0, lines


# ---------------------------------------------------------------------------
# Self-test. Each case builds its OWN repository inside a mktemp, never the real
# tree: a self-test that depends on the working tree passes or fails according to
# who runs it, which is the opposite of a self-test.
# ---------------------------------------------------------------------------

HEADER = "// Author: Julian Bolivar\n// Version: %s\n// Date: 2026-01-01\n\npub fn f() {}\n"


def _repo(tmp, manifest="4.1.0"):
    root = Path(tmp)
    (root / "src").mkdir(parents=True)
    (root / "Cargo.toml").write_text('version = "%s"\n' % manifest, encoding="utf-8")
    git(["init", "-q"], cwd=root)
    git(["config", "user.email", "t@t"], cwd=root)
    git(["config", "user.name", "t"], cwd=root)
    return root


def _commit(root, message="c"):
    git(["add", "-A"], cwd=root)
    git(["commit", "-qm", message], cwd=root)


def self_test():
    failures = []

    def case(name, want_code, build, want_text=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = build(tmp)
            code, lines = check(root)
            blob = "\n".join(lines)
            ok = code == want_code and (want_text is None or want_text in blob)
            print("  [%s] %-42s" % ("ok" if ok else "FAIL", name))
            if not ok:
                failures.append("%s: code=%s out=%r" % (name, code, blob))

    def modified(tmp):
        root = _repo(tmp)
        (root / "src" / "a.rs").write_text(HEADER % "4.0.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.0.0"], cwd=root)
        (root / "src" / "a.rs").write_text(HEADER % "4.0.0" + "// edit\n", encoding="utf-8")
        _commit(root, "edit")
        return root
    case("1  modified with a stale header", 1, modified, "header says 4.0.0")

    def synced(tmp):
        root = modified(tmp)
        (root / "src" / "a.rs").write_text(HEADER % "4.1.0" + "// edit\n", encoding="utf-8")
        _commit(root, "sync")
        return root
    case("2  modified and synced", 0, synced, "OK")

    def deleted(tmp):
        root = _repo(tmp)
        (root / "src" / "a.rs").write_text(HEADER % "4.1.0", encoding="utf-8")
        (root / "src" / "b.rs").write_text(HEADER % "4.1.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.0.0"], cwd=root)
        git(["rm", "-q", "src/b.rs"], cwd=root)
        _commit(root, "delete")
        return root
    case("3  deleted path is not opened", 0, deleted, "OK")

    def renamed(tmp):
        root = _repo(tmp)
        (root / "src" / "old.rs").write_text(HEADER % "4.0.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.0.0"], cwd=root)
        git(["mv", "src/old.rs", "src/new.rs"], cwd=root)
        _commit(root, "rename")
        return root
    case("4  clean rename checks the DESTINATION", 1, renamed, "src/new.rs")

    def rewritten(tmp):
        root = _repo(tmp)
        (root / "src" / "old.rs").write_text(HEADER % "4.0.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.0.0"], cwd=root)
        git(["rm", "-q", "src/old.rs"], cwd=root)
        # `git rm` takes the now-empty directory with it, so recreate it.
        (root / "src").mkdir(exist_ok=True)
        (root / "src" / "new.rs").write_text(
            HEADER % "4.0.0" + "// completely different body\n" * 40, encoding="utf-8")
        _commit(root, "rewrite")
        return root
    case("5  rename below -M threshold still seen", 1, rewritten, "src/new.rs")

    def unreadable(tmp):
        root = _repo(tmp)
        (root / "src" / "a.rs").write_text(HEADER % "not-a-version", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.0.0"], cwd=root)
        (root / "src" / "a.rs").write_text(HEADER % "not-a-version" + "// e\n", encoding="utf-8")
        _commit(root, "edit")
        return root
    case("6  unparseable header FAILS", 1, unreadable, "does not parse")

    def numeric(tmp):
        root = _repo(tmp, manifest="3.10.0")
        (root / "src" / "a.rs").write_text(HEADER % "3.9.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v3.9.0"], cwd=root)
        (root / "src" / "a.rs").write_text(HEADER % "3.9.0" + "// e\n", encoding="utf-8")
        _commit(root, "edit")
        return root
    case("7  3.10.0 vs 3.9.0 compares numerically", 1, numeric, "header says 3.9.0")

    def no_tags(tmp):
        root = _repo(tmp)
        (root / "src" / "a.rs").write_text(HEADER % "4.1.0", encoding="utf-8")
        _commit(root, "base")
        return root
    case("8  no tags FAILS CLOSED", 1, no_tags, "no reachable")

    def pin_stale(tmp):
        root = _repo(tmp)
        (root / "src" / "consensus.rs").write_text(HEADER % "1.0.0", encoding="utf-8")
        (root / "src" / "a.rs").write_text(HEADER % "4.1.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.0.0"], cwd=root)
        (root / "src" / "a.rs").write_text(HEADER % "4.1.0" + "// e\n", encoding="utf-8")
        _commit(root, "edit")
        return root
    case("9a pin catches drift from before the tag", 1, pin_stale, "consensus.rs")

    def pin_off(tmp):
        root = _repo(tmp)
        (root / "src" / "consensus.rs").write_text(HEADER % "4.1.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.0.0"], cwd=root)
        return root
    case("9b pin switches itself off when repaired", 0, pin_off, "OK")

    # 11. TWO TAGS spanning the 9 -> 10 boundary. This is the case the docstring's
    #     headline claim needs and case 7 did NOT provide: case 7 exercises
    #     `got != want`, which is order-independent, so a TEXT comparison satisfies
    #     it identically. Ordering only matters where a tag is CHOSEN, and picking
    #     v4.9.0 over v4.10.0 by text silently disarms the pin -- a pinned file whose
    #     header reads 4.9.x then satisfies `got < (4, 9, 0) == False` and is skipped,
    #     which is the one direction the pin exists to cover.
    def two_tags(tmp):
        root = _repo(tmp, manifest="4.10.0")
        (root / "src" / "consensus.rs").write_text(HEADER % "4.9.0", encoding="utf-8")
        _commit(root, "base")
        git(["tag", "v4.9.0"], cwd=root)
        git(["tag", "v4.10.0"], cwd=root)
        return root
    case("11 v4.10.0 beats v4.9.0, so the pin fires", 1, two_tags, "consensus.rs")

    def absent_root(tmp):
        root = pin_off(tmp)
        return root
    case("10 absent root is SAID, not silent", 0, absent_root, "SKIP: root benches")

    if failures:
        print("\nSELF-TEST FAILED:")
        for line in failures:
            print("  " + line)
        return 1
    print("\nSELF-TEST OK -- 12 cases")
    return 0


def main():
    if "--self-test" in sys.argv[1:]:
        return self_test()
    code, lines = check()
    for line in lines:
        print(line)
    return code


if __name__ == "__main__":
    sys.exit(main())
