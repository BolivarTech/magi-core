#!/usr/bin/env python3
# Author: Julian Bolivar
# Version: 1.0.1
# Date: 2026-09-14
#
# Identifying field names must not reach a tracked fixture.
#
# MS2 captures real CLI envelopes and tracks them as fixtures. They carry
# `session_id`, `uuid`, `total_cost_usd` and `inference_geo`: not credentials, so
# ADR 009 does not reach them, but they identify a session and an account and they
# stay in git history forever. The redaction rule is procedural; this is its guard,
# because a procedural discipline fails silently -- whoever captures the next
# envelope six months from now will not read that paragraph.
#
# SCOPE, declared because a guard that looks wider than it is beats no guard only
# when its limits are written: it matches KEY NAMES, recursively, at any depth. It
# does not scan VALUES, so an identifier pasted inside a free-text field passes.
# That half stays with review.
#
# ORDER, and both halves matter:
#   1. If the root EXISTS on disk, scan it -- tracked or not. A capture that has
#      not been `git add`ed yet is the highest-risk moment, so consulting git first
#      and skipping the untracked would invert this guard's purpose.
#   2. Only when the root is ABSENT does git decide: `git ls-tree` in HEAD does not
#      know it => there were never captures, SKIP with its reason; git knows it and
#      disk does not => someone deleted it, FAIL.
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

# The root MS2 actually writes to. It is `src/`, not `tests/`, and the reason is
# measured rather than stylistic: `src/test_support.rs` is gated on
# `cfg(any(test, feature = "test-utils"))`, so it is a NORMAL compiled module under
# that feature -- `cargo build --features test-utils` expands its `include_str!`
# without `cfg(test)` ever being set. R-32 excludes `tests/` from the package, so
# fixtures living there would make that build fail against the published tarball.
FIXTURE_ROOT = Path("src/providers/fixtures/envelopes")

# WHITELIST, not blacklist, and the direction of failure is the point: a key nobody
# anticipated is REJECTED until someone declares it consumed, and declaring it is
# exactly when one asks whether it should travel at all. A blacklist fails the other
# way -- the same class as 2.2.0's enumerated invisible code points, where the next
# unlisted one is the one that leaks.
#
# QUALIFIED PATHS from the object root, never bare names. Bare names have two holes:
# an `output_tokens` hanging off the ROOT would pass by resembling the one under
# `usage`, and the case that matters -- an unconsumed key under a PERMITTED parent,
# like `usage.prompt_tokens` -- would be undecided. `usage` is whitelisted as a NODE,
# not as a wildcard over its subtree.
# Qualified paths, and ARRAY POSITION IS PART OF THE PATH: the walker renders an
# array element as `[]`, so a consumed field nested inside one -- `iterations[]
# .usage` rather than `usage` -- is NOT in this set and the guard refuses it.
#
# Deliberate, and the fail-closed direction, with a cost worth stating: a future
# capture whose shape nests a consumed field inside an array gets a RED on
# legitimate content, and the fix is to add the qualified form here ON PURPOSE.
# A whitelist that quietly accepted any nesting would accept an identifying field
# in one too, which is the whole reason the paths are qualified -- case 8 pins
# exactly that, catching `permission_denials[].session_id`.
CONSUMED = frozenset({
    "stop_reason", "usage", "usage.output_tokens",
    "api_error_status", "is_error", "result",
})

# Kept as a MESSAGE, never as a rule: when the offender is one of these, say so.
# Two rules for one invariant is how one of them gets updated and the other does not.
KNOWN_IDENTIFYING = frozenset({
    "session_id", "uuid", "total_cost_usd", "inference_geo",
})

# Rule names. Every finding carries the rule that produced it, so a self-test case
# cannot pass because a DIFFERENT rule happened to fire.
RULE_IDENTIFYING = "KNOWN_IDENTIFYING"
# NOT "CONSUMED": that is the string the PASS branch returns, so a failure printed
# `FAIL (CONSUMED)` and a self-test asserting `want_rule="CONSUMED"` was satisfied by
# any finding at all. A rule name that a PASS can also produce is not a rule name.
RULE_GIT_FAILED = "GIT_UNAVAILABLE"
RULE_UNCONSUMED = "KEY_NOT_CONSUMED"
RULE_UNREADABLE = "UNREADABLE_FIXTURE"
RULE_ROOT_GONE = "ROOT_TRACKED_BUT_ABSENT"
RULE_ROOT_NEVER = "ROOT_NEVER_EXISTED"


def key_paths(node, prefix=""):
    """Yield the qualified path of every key reachable from ``node``.

    Arrays are walked and their INDEX is elided: element 0 and element 7 both
    yield ``permission_denials[]``. A walker that only recurses into objects is
    the widest hole this guard can have -- the CLI envelope carries list-valued
    fields, so one `session_id` inside a list element would never be visited.
    Eliding the index keeps the whitelist a set of SHAPES rather than of
    positions; indexing would make the check depend on how many elements a
    capture happened to have, so a fixture would pass or fail by length.

    Complexity: O(n) in the number of nodes, each visited once.
    """
    if isinstance(node, dict):
        for key, value in node.items():
            path = "%s.%s" % (prefix, key) if prefix else key
            yield path
            for deeper in key_paths(value, path):
                yield deeper
    elif isinstance(node, list):
        # A list is a container, never a leaf: it is the elements that get compared.
        for element in node:
            for deeper in key_paths(element, prefix + "[]"):
                yield deeper


def scan_file(path):
    """Return the findings for one fixture file, as ``(rule, detail)`` pairs."""
    if path.suffix != ".json":
        # The fixture root holds JSON envelopes and nothing else. Anything else is
        # an error, not an exception: a guard that SKIPS what it cannot parse has
        # the easiest hole to hit by accident.
        return [(RULE_UNREADABLE, "%s: not a .json file" % path.as_posix())]
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (ValueError, OSError) as err:
        return [(RULE_UNREADABLE, "%s: %s" % (path.as_posix(), err))]

    findings = []
    for path_str in key_paths(data):
        if path_str in CONSUMED:
            continue
        last = path_str.rsplit(".", 1)[-1]
        if last in KNOWN_IDENTIFYING:
            findings.append((RULE_IDENTIFYING,
                             "%s: identifying field left in a fixture: %s"
                             % (path.as_posix(), path_str)))
        else:
            findings.append((RULE_UNCONSUMED,
                             "%s: key not in the consumed set: %s"
                             % (path.as_posix(), path_str)))
    return findings


def git_knows(root):
    """Whether ``root`` is registered in HEAD: True, False, or None if git failed.

    NONE IS NOT FALSE, and collapsing them is how this guard would approve by
    starvation. Reading only stdout, a git that errored -- no commits yet, a
    corrupt index, git absent -- returns empty output, which is indistinguishable
    from "the tree does not know this path"; the caller then reports SKIP and the
    round gate stays green having scanned nothing. The caller treats None as a
    hard failure.

    Its sibling `check_header_sync.py` had the identical defect in `changed_since`
    and it was fixed there one round earlier without auditing this file -- the
    dimension was named and its sibling was not.
    """
    try:
        # NO COMMITS is not a broken git, and `ls-tree HEAD` fails identically in
        # both, so the two are separated before anything is decided. Conflating
        # them costs one direction or the other: treat both as broken and the only
        # legitimate SKIP becomes unreachable -- a tree with no captures yet, which
        # is this guard's expected state until MS2 lands; treat both as "no
        # commits" and a broken git reports nothing to find.
        #
        # MEASURED rather than reasoned, because the obvious probe does not work:
        # `rev-parse --verify HEAD` fails in BOTH (rc 1 fresh, rc 128 broken), so
        # it cannot tell them apart. `rev-list --all --count` can -- rc 0 with "0"
        # on a fresh repo, non-zero when git cannot read its own object store.
        commits = subprocess.run(["git", "rev-list", "--all", "--count"],
                                 capture_output=True, text=True, check=False)
        if commits.returncode != 0:
            return None
        if commits.stdout.strip() == "0":
            return False
        out = subprocess.run(
            ["git", "ls-tree", "-d", "--name-only", "HEAD", root.as_posix()],
            capture_output=True, text=True, check=False)
    except OSError:
        return None
    if out.returncode != 0:
        return None
    return bool(out.stdout.strip())


def check(root=FIXTURE_ROOT):
    """Run the guard. Returns ``(exit_code, rule, lines)``.

    ``exit_code`` is 0 for PASS and for the declared SKIP, 1 for FAIL.
    """
    if not root.is_dir():
        # Only here does git decide, and the THREE answers mean different things.
        # None is git having failed, and it fails CLOSED: a guard that could not
        # ask cannot report that there is nothing to find.
        tracked = git_knows(root)
        if tracked is None:
            return (1, RULE_GIT_FAILED,
                    ["git could not say whether %s is tracked -- refusing to "
                     "report on a scan that never happened" % root.as_posix()])
        if tracked:
            return (1, RULE_ROOT_GONE,
                    ["%s is registered in git but absent from disk" % root.as_posix()])
        return (0, RULE_ROOT_NEVER,
                ["SKIP: %s does not exist and git never knew it -- no captures yet"
                 % root.as_posix()])

    findings = []
    for entry in sorted(root.rglob("*")):
        if entry.is_file():
            findings.extend(scan_file(entry))
    if findings:
        rules = sorted({rule for rule, _ in findings})
        return (1, "+".join(rules), [detail for _, detail in findings])
    return (0, "ALL_KEYS_CONSUMED", ["PASS: every key under %s is in the consumed set"
                            % root.as_posix()])


# ---------------------------------------------------------------------------
# Self-test. Every case names the RULE it expects, so a PASS cannot come from a
# rule other than the one the case exercises.
# ---------------------------------------------------------------------------

CLEAN = {"stop_reason": "end_turn", "is_error": False, "result": "hi",
         "usage": {"output_tokens": 5}}

CASES = [
    # (name, files, expect_code, expect_rule, expect_substring)
    ("1  clean fixture", {"ok.json": CLEAN}, 0, "ALL_KEYS_CONSUMED", "PASS"),
    ("2  nested identifying field", {"bad.json": {"usage": {"session_id": "x"}}},
     1, RULE_IDENTIFYING, "identifying field left in a fixture"),
    ("2b unconsumed but harmless key", {"d.json": {"duration_ms": 12}},
     1, RULE_UNCONSUMED, "duration_ms"),
    ("2c unconsumed under a permitted parent",
     {"u.json": {"usage": {"output_tokens": 5, "prompt_tokens": 9}}},
     1, RULE_UNCONSUMED, "usage.prompt_tokens"),
    ("2d consumed name at the wrong depth", {"r.json": {"output_tokens": 5}},
     1, RULE_UNCONSUMED, "key not in the consumed set: output_tokens"),
    ("2e identifying field inside an array",
     {"a.json": {"permission_denials": [{"session_id": "x"}]}},
     1, RULE_IDENTIFYING, "permission_denials[].session_id"),
    ("2f array path carries no index",
     {"b.json": {"permission_denials": [{}, {"uuid": "x"}]}},
     1, RULE_IDENTIFYING, "permission_denials[].uuid"),
]


def _write(root, files):
    root.mkdir(parents=True, exist_ok=True)
    for name, payload in files.items():
        (root / name).write_text(json.dumps(payload), encoding="utf-8")


def self_test():
    failures = []
    for name, files, want_code, want_rule, want_text in CASES:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "src" / "providers" / "fixtures" / "envelopes"
            _write(root, files)
            code, rule, lines = check(root)
            blob = "\n".join(lines)
            ok = code == want_code and want_text in blob
            if want_rule is not None:
                ok = ok and want_rule in rule
            print("  [%s] %-40s rule=%s" % ("ok" if ok else "FAIL", name, rule))
            if not ok:
                failures.append("%s: code=%s rule=%s out=%r" % (name, code, rule, blob))

    # 2g. Unparseable and non-JSON files are FAILURES, each named. A guard that
    # skips what it cannot parse lets a fixture with one stray comma into history.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "src" / "providers" / "fixtures" / "envelopes"
        root.mkdir(parents=True)
        (root / "notes.md").write_text("not json", encoding="utf-8")
        (root / "roto.json").write_text("{,}", encoding="utf-8")
        code, rule, lines = check(root)
        blob = "\n".join(lines)
        ok = code == 1 and RULE_UNREADABLE in rule and "notes.md" in blob and "roto.json" in blob
        print("  [%s] %-40s rule=%s" % ("ok" if ok else "FAIL", "2g unreadable files", rule))
        if not ok:
            failures.append("2g: code=%s rule=%s out=%r" % (code, rule, blob))

    # 3. Root absent and git never knew it -> SKIP with its reason, never a mute OK.
    with tempfile.TemporaryDirectory() as tmp:
        cwd = os.getcwd()
        try:
            os.chdir(tmp)
            subprocess.run(["git", "init", "-q"], check=False, capture_output=True)
            code, rule, lines = check(Path("src/providers/fixtures/envelopes"))
            blob = "\n".join(lines)
            ok = code == 0 and rule == RULE_ROOT_NEVER and "SKIP" in blob
        finally:
            os.chdir(cwd)
        print("  [%s] %-40s rule=%s" % ("ok" if ok else "FAIL", "3  absent, untracked", rule))
        if not ok:
            failures.append("3: code=%s rule=%s out=%r" % (code, rule, blob))

    # 3b. Git present but BROKEN, root absent -> FAIL, where 3 SKIPs. The two look
    #     identical from stdout alone (both empty), which is why 3 alone could not
    #     pin this: reading only stdout, a git that errored reported "never knew
    #     it" and the gate went green having scanned nothing.
    with tempfile.TemporaryDirectory() as tmp:
        cwd = os.getcwd()
        try:
            os.chdir(tmp)
            # Auto-maintenance is switched off for this throwaway repo: a git that
            # runs it in the background leaves `.git/objects/maintenance.lock` for
            # a moment, and the rmtree below raced against it on a CI runner --
            # the lock vanished between the directory listing and the unlink, and
            # the self-test died on a file that was never the subject.
            subprocess.run(["git", "init", "-q"], check=False, capture_output=True)
            subprocess.run(["git", "config", "maintenance.auto", "false"],
                           check=False, capture_output=True)
            subprocess.run(["git", "config", "gc.auto", "0"],
                           check=False, capture_output=True)
            Path("f.txt").write_text("x", encoding="utf-8")
            subprocess.run(["git", "add", "-A"], check=False, capture_output=True)
            subprocess.run(["git", "-c", "user.email=t@t", "-c", "user.name=t",
                            "commit", "-qm", "c"], check=False, capture_output=True)
            # HEAD resolves, then the object graph is emptied so `ls-tree` fails.
            def _force(func, path, _exc):
                # A path that disappeared on its own is already what rmtree wanted;
                # anything else is retried once with the read-only bit cleared.
                if not os.path.lexists(path):
                    return
                os.chmod(path, stat.S_IWRITE)
                func(path)
            shutil.rmtree(Path(".git") / "objects", onerror=_force)
            code, rule, lines = check(Path("src/providers/fixtures/envelopes"))
            blob = "\n".join(lines)
            ok = code == 1 and rule == RULE_GIT_FAILED
        finally:
            os.chdir(cwd)
        print("  [%s] %-40s rule=%s" % ("ok" if ok else "FAIL",
                                        "3b broken git FAILS, not SKIP", rule))
        if not ok:
            failures.append("3b: code=%s rule=%s out=%r" % (code, rule, blob))

    # 4. Root registered in git and deleted from disk -> FAIL. This is the half
    # that keeps an empty scan set from reading green (the R-29 lesson).
    with tempfile.TemporaryDirectory() as tmp:
        cwd = os.getcwd()
        try:
            os.chdir(tmp)
            root = Path("src/providers/fixtures/envelopes")
            _write(root, {"ok.json": CLEAN})
            for cmd in (["git", "init", "-q"],
                        ["git", "add", "-A"],
                        ["git", "-c", "user.email=a@b", "-c", "user.name=a",
                         "commit", "-q", "-m", "x"]):
                subprocess.run(cmd, check=False, capture_output=True)
            (root / "ok.json").unlink()
            root.rmdir()
            code, rule, lines = check(root)
            blob = "\n".join(lines)
            ok = code == 1 and rule == RULE_ROOT_GONE
        finally:
            os.chdir(cwd)
        print("  [%s] %-40s rule=%s" % ("ok" if ok else "FAIL", "4  tracked, deleted", rule))
        if not ok:
            failures.append("4: code=%s rule=%s out=%r" % (code, rule, blob))

    if failures:
        print("\nSELF-TEST FAILED:")
        for line in failures:
            print("  " + line)
        return 1
    print("\nSELF-TEST OK -- 11 cases")
    return 0


def main():
    if "--self-test" in sys.argv[1:]:
        return self_test()
    code, rule, lines = check()
    for line in lines:
        print(line)
    if code:
        print("FAIL (%s)" % rule)
    return code


if __name__ == "__main__":
    sys.exit(main())
