#!/usr/bin/env python3
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-09-05
#
# Every SILENT change must be named in the CHANGELOG section of its own version.
#
# A silent change is one that COMPILES the same and behaves differently, so the
# compiler cannot warn and the rustdoc only reaches whoever reads it. The
# CHANGELOG entry is then the ONLY channel that reaches a consumer who matches by
# name -- which is why this floor exists and why it goes ROW BY ROW.
#
# ROW BY ROW, never a bag of tokens: a grep over a flat list lets one row HIDE
# BEHIND another's token -- `ProviderError::Http` appears for R-3 and would cover
# R-2. Fifteen independent conditions, each naming the row it failed.
#
# SCOPED to the current version's section. Without scoping, the tokens are found
# in any older entry and the floor passes while THIS version says nothing -- a
# guard approving by looking at the past.
#
# EVERY TOKEN IS AN IDENTIFIER, never a common word. `window` and `finish` would
# be satisfied by ordinary prose ("the context window", "we finish by"), so a
# floor built from them is satisfied by accident.
#
# WHAT THIS DOES NOT DO, said so the floor is not read as wider than it is: a grep
# catches the total omission, not a mention that fails to EXPLAIN. That half stays
# with the person writing the release. And one row of the contract's sixteen is
# NOT here at all -- R-1's derivation caveat (`register_transport_failure`) is
# verified by READING, because what it asserts is that a function has a single
# production caller, which no grep over prose can answer.
import re
import sys
import tempfile
from pathlib import Path

# (row, kind, tokens)
#   "all"     -> every token must appear somewhere in the version section
#   "cooccur" -> every token must appear inside the SAME subsection
DISCLOSURES = [
    ("R-1  the abandoned-retry error keeps its original class",
     "all", ["RetryAbandoned"]),
    # The row LABEL deliberately avoids the literal token: the self-test builds its
    # fixture from these labels, and a label carrying its own token would satisfy the
    # search after the token was removed -- a fixture that cannot go red.
    ("R-2  the overloaded status becomes retryable", "all", ["529"]),
    ("R-3  CLI failures move from Process to Http",
     "all", ["ProviderError::Process", "ProviderError::Http"]),
    ("R-4  a replaced primary clears its declared probe",
     "all", ["with_provider"]),
    ("R-5  a recovered run no longer aborts", "all", ["EndpointDown"]),
    ("R-6  the preflight keeps a measurement it already paid for",
     "all", ["run_preflight"]),
    ("R-8  the summary carries the emitted side", "all", ["majority_summary"]),
    ("R-8  confidence sums the emitted side", "all", ["ConsensusResult::confidence"]),
    ("R-8  dissent lists other agents", "all", ["ConsensusResult::dissent"]),
    ("R-9  schema failures stop being InvalidJson", "all", ["MalformedObject"]),
    ("R-22 sources stops repeating an agent", "all", ["DedupFinding::sources"]),
    # Recovered from the contract: the plan's table had thirteen rows and §2 9b has
    # sixteen. These two were the grep-able ones missing.
    ("R-23 the HOLD label stops reading as a rejection win",
     "all", ["ConsensusResult::consensus"]),
    ("R-26 the CLI reports what the backend said",
     "all", ["FinishReason", "completion_tokens"]),
    ("R-32 tests, .github and ci leave the package", "all", ["exclude = ["]),
    # COMPOUND and CO-OCCURRENT: two loose greps over the whole section would be
    # satisfied by any pair of sentences -- `Process.stderr` from R-3's own row and
    # `diagnosis` from anywhere -- without a row that says BOTH things. Same
    # SUBSECTION, not same LINE: `humanize` runs before this floor and reflows
    # prose, so a line-scoped guard reddens a correct CHANGELOG.
    ("R-3  the CONTENT of Process.stderr changes",
     "cooccur", ["Process.stderr", "diagnosis"]),
]

# Not silent -- the compiler announces them -- but they BREAK a strict build, so
# they reach a consumer as a broken build rather than as a warning. Scoped to the
# `### Deprecated` subsection, which is what keeps `majority_summary` from
# satisfying its own row and its deprecation with one mention.
DEPRECATIONS = [
    ("new", "OpenAiCompatibleProvider::new"),
    ("with_timeout", "OpenAiCompatibleProvider::with_timeout"),
    ("majority_summary", "majority_summary"),
    ("ZERO_WIDTH_PATTERN", "ZERO_WIDTH_PATTERN"),
]

DEPRECATED_HEADING = "### Deprecated"

# THE LIST DESCRIBES ONE RELEASE, so it only judges that release. Without this the
# guard demands 4.1.0's nineteen disclosures from EVERY future version: the 4.2.0
# release engineer meets a red naming defects that are not theirs, and the two
# cheapest ways out are emptying the list (retiring the guard with no decision) or
# copying 4.1.0's rows into 4.2.0's section (a false record). A guard that is red by
# default is a guard that gets ignored.
#
# It SKIPS loudly rather than passing quietly: a silent OK on another version would
# read as "checked and clean", which is the opposite of what happened. When the next
# release needs a floor, it gets its own list and its own APPLIES_TO.
APPLIES_TO = "4.1.0"


def version_section(changelog_text, version):
    """The `## [version]` section, up to the next `## [`. ``None`` when absent."""
    start = re.search(r"^## \[%s\]" % re.escape(version), changelog_text, re.M)
    if not start:
        return None
    rest = changelog_text[start.end():]
    nxt = re.search(r"^## \[", rest, re.M)
    return rest[:nxt.start()] if nxt else rest


def subsections(section_text):
    """Split a version section into ``{heading: body}`` by its `###` headings.

    The PREAMBLE -- anything written before the first `###` -- is deliberately not a
    subsection, which puts a structural requirement on whoever writes the CHANGELOG:
    the compound row and the deprecations must live under a heading, not loose at the
    top. It fails CLOSED (the row is reported missing rather than silently accepted),
    and it is said here because an undocumented requirement is one someone meets by
    accident.
    """
    out = {}
    parts = re.split(r"^(### .*)$", section_text, flags=re.M)
    for i in range(1, len(parts), 2):
        out[parts[i].strip()] = parts[i + 1]
    return out


def check(changelog=Path("CHANGELOG.md"), manifest=Path("Cargo.toml")):
    """Run the floor. Returns ``(exit_code, lines)``."""
    m = re.search(r'^version\s*=\s*"([^"]+)"', manifest.read_text(encoding="utf-8"), re.M) \
        if manifest.is_file() else None
    if not m:
        return 1, ["FAIL: no parseable `version` in %s" % manifest.as_posix()]
    version = m.group(1)

    if version != APPLIES_TO:
        return 0, ["SKIP: this disclosure list describes %s and the tree is at %s -- "
                   "not checked. A release needs its own list; see APPLIES_TO."
                   % (APPLIES_TO, version)]

    text = changelog.read_text(encoding="utf-8") if changelog.is_file() else ""
    section = version_section(text, version)
    if section is None:
        # FAIL, never PASS by emptiness. An absent section means this version
        # discloses nothing, which is the loudest possible failure of this floor.
        return 1, ["FAIL: %s has no `## [%s]` section -- nothing to check, which is a "
                   "failure and not a pass" % (changelog.as_posix(), version)]

    subs = subsections(section)
    missing = []

    # The disclosure rows are searched against the section MINUS the `### Deprecated`
    # body. Without that subtraction the `majority_summary` row is satisfied by its own
    # deprecation line: the round's single silent content change could go unmentioned
    # while the floor reported OK, and the comment above claimed the scoping prevented
    # exactly that. It prevented one direction only.
    body_only = section
    if DEPRECATED_HEADING in subs:
        body_only = section.replace(subs[DEPRECATED_HEADING], "")

    for row, kind, tokens in DISCLOSURES:
        if kind == "all":
            gone = [t for t in tokens if t not in body_only]
            if gone:
                missing.append("%s -- missing: %s" % (row, ", ".join(gone)))
        else:
            if not any(all(t in body for t in tokens) for body in subs.values()):
                missing.append("%s -- no single subsection carries both: %s"
                               % (row, " AND ".join(tokens)))

    deprecated_body = subs.get(DEPRECATED_HEADING)
    if deprecated_body is None:
        missing.append("`%s` subsection is absent from `## [%s]` -- the four "
                       "deprecations have nowhere to be named" % (DEPRECATED_HEADING, version))
    else:
        for name, token in DEPRECATIONS:
            if token not in deprecated_body:
                missing.append("deprecation `%s` -- missing `%s` from `%s`"
                               % (name, token, DEPRECATED_HEADING))

    if missing:
        lines = ["FAIL: %d finding(s) in `## [%s]` -- disclosure rows and deprecations "
                 "are counted together here; each is named below"
                 % (len(missing), version)]
        lines.extend("  " + item for item in missing)
        lines.append("NOTE: R-1's derivation caveat (`register_transport_failure`) is NOT "
                     "checked here -- it is verified by reading, not by grep.")
        return 1, lines
    return 0, ["OK: %d disclosure row(s) and %d deprecation(s) named in `## [%s]`"
               % (len(DISCLOSURES), len(DEPRECATIONS), version),
               "NOTE: a grep catches the total omission, not a mention that fails to "
               "explain; and R-1's derivation caveat is verified by reading."]


# ---------------------------------------------------------------------------
# Self-test.
# ---------------------------------------------------------------------------

def _full_section(version="4.1.0"):
    body = ["## [%s] - 2026-09-05" % version, "", "### Changed", ""]
    for row, kind, tokens in DISCLOSURES:
        if kind == "cooccur":
            continue
        body.append("- %s: %s." % (row, " and ".join("`%s`" % t for t in tokens)))
    body += ["", "### Changed content", "",
             "- The `Process.stderr` field now travels labelled, and that label is a",
             "  diagnosis rather than the process's own stderr.", "",
             DEPRECATED_HEADING, ""]
    for name, token in DEPRECATIONS:
        body.append("- `%s` is deprecated and goes away in the next major." % token)
    body += ["", "## [4.0.0] - 2026-08-24", "", "- older entry"]
    return "\n".join(body) + "\n"


def self_test():
    failures = []

    def case(name, want_code, changelog, want_text=None, manifest='version = "4.1.0"\n'):
        with tempfile.TemporaryDirectory() as tmp:
            cl = Path(tmp) / "CHANGELOG.md"
            mf = Path(tmp) / "Cargo.toml"
            cl.write_text(changelog, encoding="utf-8")
            mf.write_text(manifest, encoding="utf-8")
            code, lines = check(cl, mf)
            blob = "\n".join(lines)
            ok = code == want_code and (want_text is None or want_text in blob)
            print("  [%s] %-52s" % ("ok" if ok else "FAIL", name))
            if not ok:
                failures.append("%s: code=%s out=%r" % (name, code, blob))

    full = _full_section()
    case("1  complete section", 0, full, "OK")

    # 2. One row removed -> FAIL naming it.
    case("2  one row missing is named", 1,
         full.replace("`EndpointDown`", "`SomethingElse`"), "R-5")

    # 3. Tokens only in an OLD section -> FAIL. Without scoping, the floor
    #    approves by looking at the past.
    old_only = full.replace("## [4.1.0] - 2026-09-05", "## [3.9.0] - 2026-01-01")
    case("3  tokens only in an older section", 1, old_only, "has no `## [4.1.0]`")

    # 4. No section for the current version -> FAIL, not PASS by emptiness.
    case("4  absent version section is a FAILURE", 1,
         "## [4.0.0] - 2026-08-24\n\n- older entry\n", "nothing to check")

    # 5. A row satisfied through ANOTHER row's token: R-3 supplies
    #    `ProviderError::Http`; R-2's `529` is still missing and must fail.
    case("5  a row cannot hide behind another's token", 1,
         full.replace("`529`", "five hundred and twenty nine"), "R-2")

    # 6. TABLE-DRIVEN over all FOUR deprecations, not a sample: the row nobody
    #    picked is the one that breaks.
    for name, token in DEPRECATIONS:
        broken = full.replace(
            "- `%s` is deprecated and goes away in the next major.\n" % token, "")
        case("6  deprecation `%s` removed is named" % name, 1, broken, name)

    # 7. `### Deprecated` absent entirely -> FAIL naming the subsection.
    case("7  absent Deprecated subsection is named", 1,
         full.replace(DEPRECATED_HEADING, "### Notes"), "subsection is absent")

    # 8. COLLISION: `majority_summary` in the body (R-8's row) but NOT inside
    #    `### Deprecated`. The row PASSES and the deprecation FAILS, separately.
    collided = full.replace(
        "- `majority_summary` is deprecated and goes away in the next major.\n", "")
    case("8  body mention does not satisfy the deprecation", 1, collided,
         "deprecation `majority_summary`")

    # 9. REFLOW: the compound row split across lines still passes. `humanize`
    #    runs before this floor, and a legitimate reflow must not redden it.
    reflowed = full.replace(
        "- The `Process.stderr` field now travels labelled, and that label is a\n"
        "  diagnosis rather than the process's own stderr.",
        "- The `Process.stderr` field now travels\n  labelled, and that label is a\n"
        "  diagnosis rather than the\n  process's own stderr.")
    case("9  a reflowed compound row still passes", 0, reflowed, "OK")

    # 8b. THE MIRROR of case 8, and the one that was missing: the deprecation line
    #     present but the CONTENT-change row absent. Before the subtraction above,
    #     `majority_summary` in `### Deprecated` satisfied the row too, so the single
    #     silent content break of this round could go unmentioned with the floor green.
    no_row = full.replace(
        "- R-8  the summary carries the emitted side: `majority_summary`." + "\n", "")
    case("8b deprecation alone does not satisfy the row", 1, no_row,
         "R-8  the summary carries the emitted side")

    # 10. ANOTHER VERSION -> SKIP, loudly. This is the case that pins the scoping:
    #     without APPLIES_TO the list is demanded of every future release, and the
    #     4.2.0 engineer meets a red naming defects that are not theirs.
    other = "## [4.2.0] - 2026-12-01" + "\n\n" + "### Added" + "\n\n" + "- An ordinary feature." + "\n"
    case("10 another version SKIPs, loudly", 0, other, "SKIP",
         manifest='version = "4.2.0"\n')

    if failures:
        print("\nSELF-TEST FAILED:")
        for line in failures:
            print("  " + line)
        return 1
    print("\nSELF-TEST OK -- 14 cases")
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
