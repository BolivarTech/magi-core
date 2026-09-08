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
    # Added by MS2, in the same commit that introduces the constant. A guard row and
    # the thing it guards arrive together: adding the row at closing time leaves a
    # window in which the disclosure exists and nothing watches it, and in that
    # window the floor passes green over a row nobody is checking.
    ("R-10 a failed prompt write is labelled, never a bare io error",
     "all", ["label_prompt_write_diagnosis"]),
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
# A DROPPED MILESTONE PRUNES ROWS, and this guard has no mechanism for it -- said
# here because the alternative is a release engineer meeting a red on a CORRECT
# CHANGELOG with nothing in the file to explain it, whose cheapest escape is emptying
# the list. That is the same "guard retired with no decision" the scoping below was
# written to prevent, reached by another door.
#
# Contract §1.1(e): if a milestone does not land, the rows its REQ produce have no
# entry to be named in, so they come OUT of this list in the same reported decision
# that drops the milestone. The correspondence row -> milestone is direct: R-1/R-2 are
# MS1, R-3/R-26 MS2, R-4/R-5/R-6 MS3, R-8/R-9/R-22/R-23 MS4, R-7's deprecations MS5,
# R-21's deprecation MS6, R-32 MS0. Pruning is mechanical; what is inadmissible is
# pruning a row whose REQ DID land.
APPLIES_TO = "4.1.0"

# Versions EXAMINED and found to carry no silent change, with the reason. This is
# the deliberate escape from the failure above: a docs-only patch does not need a
# disclosure list, but it does need someone to have said so. An empty mapping is
# the correct state until a release earns an entry.
#
# THE REASON IS REQUIRED, and enforced below rather than requested here. An escape
# that costs one word is the cheapest way to make a red go away, and the cheapest
# escape is the one a hurried person takes at release time -- which is exactly how
# a guard stops guarding. Writing why leaves a record a reviewer can disagree with;
# adding a version to a list leaves nothing.
#
# WHAT THE ENTRY IS CLAIMING, spelled out because the claim is easy to make and hard
# to remember: that this release changes NOTHING a consumer's code would compile
# through unchanged and behave differently under. Not "no API break" -- the compiler
# announces those. A field whose contents change meaning, a count that starts
# excluding duplicates, an error that stops being retryable: those compile fine and
# arrive silently, and this floor exists for them alone.
NO_DISCLOSURES = {}


def version_section(changelog_text, version):
    """The `## [version]` section, up to the next `## [`. ``None`` when absent."""
    start = re.search(r"^## \[%s\]" % re.escape(version), changelog_text, re.M)
    if not start:
        return None
    rest = changelog_text[start.end():]
    nxt = re.search(r"^## \[", rest, re.M)
    return rest[:nxt.start()] if nxt else rest


def present(token, text):
    """Whether ``token`` appears in ``text`` as a whole token, not a substring.

    `in` was the first form and it is a FALSE NEGATIVE waiting to happen: the row
    whose token is `529` is satisfied by any `5291` anywhere in the section, so a
    release could disclose nothing about the retry change and the floor would still
    report OK. The tokens are identifiers and numbers, which is exactly the shape
    substring matching gets wrong.

    The boundary is applied per END, not blanket, because several tokens are not
    bare identifiers: `exclude = [` finishes on a bracket and `Process.stderr`
    carries a dot. Demanding a word boundary beside a non-word character would
    never match. So each side gets one only when the token's own character there is
    a word character -- which is what `\\b` means, spelled out because `re.escape`
    plus a blanket `\\b` does not survive these tokens.
    """
    pat = re.escape(token)
    if token[:1].isalnum() or token[:1] == "_":
        pat = r"(?<![0-9A-Za-z_])" + pat
    if token[-1:].isalnum() or token[-1:] == "_":
        pat = pat + r"(?![0-9A-Za-z_])"
    return re.search(pat, text) is not None


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

    if version != APPLIES_TO and version not in NO_DISCLOSURES:
        # FAIL, not SKIP. Returning 0 here made this floor DISARM ITSELF on the
        # next version bump: 4.2.0 would run it, be told the list describes 4.1.0,
        # and pass having checked nothing -- a guard reporting success while the
        # thing it guards is unexamined, which is the class this milestone exists
        # to remove. It ran on the release path, so the silence landed at the one
        # moment it could not be recovered from.
        #
        # The escape is DELIBERATE rather than silent: a release with no silent
        # changes says so by name in NO_DISCLOSURES. Writing that line is a
        # decision someone makes and a reviewer can see; skipping by default was
        # neither.
        return 1, ["FAIL: this disclosure list describes %s and the tree is at %s. "
                   "A release needs its own list: either add one for %s and point "
                   "APPLIES_TO at it, or record %s in NO_DISCLOSURES to state that "
                   "it carries no silent change."
                   % (APPLIES_TO, version, version, version)]
    # THE TWO CANNOT BOTH BE TRUE, and checking it is not pedantry: with the
    # release's own version in NO_DISCLOSURES, the escape fired FIRST and returned
    # 0 while every disclosure row and deprecation went unchecked -- sixteen
    # findings on the same input without the entry. The escape was hardened to
    # cost a reason and stayed open on the flank nobody looked at: a version that
    # HAS a list cannot simultaneously claim it carries nothing to disclose.
    if APPLIES_TO in NO_DISCLOSURES:
        return 1, ["FAIL: %s is both APPLIES_TO and in NO_DISCLOSURES. A version "
                   "with a disclosure list cannot also declare it has no silent "
                   "change -- one of the two is wrong, and taking the second on "
                   "trust skips every row in the first." % APPLIES_TO]
    if version in NO_DISCLOSURES:
        why = (NO_DISCLOSURES[version] or "").strip()
        if not why:
            return 1, ["FAIL: %s is in NO_DISCLOSURES with no reason. The entry has to "
                       "say WHY the release carries no silent change -- a version added "
                       "to a list is the cheapest way to make this red go away, and an "
                       "escape that costs one word is not a decision." % version]
        return 0, ["OK: %s is recorded as carrying no silent change -- %s"
                   % (version, why)]

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
    # BUILT, not subtracted. `section.replace(subs[heading], "")` removes every
    # occurrence of that text, and an EMPTY `### Deprecated` has a body of "\n" --
    # so the subtraction stripped every newline in the section and the rows were
    # searched against one collapsed line. The tokens here are contiguous, so they
    # survived it and the damage stayed invisible; a token that ever spans a line
    # break would have gone missing with the floor reporting OK.
    #
    # Concatenation cannot do that: it selects what to search instead of deleting
    # what not to, so no content outside the excluded subsection can be touched.
    body_only = "\n".join(b for h, b in subs.items() if h != DEPRECATED_HEADING)

    for row, kind, tokens in DISCLOSURES:
        if kind == "all":
            gone = [t for t in tokens if not present(t, body_only)]
            if gone:
                missing.append("%s -- missing: %s" % (row, ", ".join(gone)))
        else:
            # The SAME subtraction the `all` rows get. Searching `subs.values()`
            # included the Deprecated body, so a compound row could be satisfied
            # from inside the block that exists to list deprecations -- the exact
            # leak the `all` rows already close, left open one branch over.
            searchable = [b for h, b in subs.items() if h != DEPRECATED_HEADING]
            if not any(all(present(t, body) for t in tokens)
                       for body in searchable):
                missing.append("%s -- no single subsection carries both: %s"
                               % (row, " AND ".join(tokens)))

    deprecated_body = subs.get(DEPRECATED_HEADING)
    if deprecated_body is None:
        missing.append("`%s` subsection is absent from `## [%s]` -- the four "
                       "deprecations have nowhere to be named" % (DEPRECATED_HEADING, version))
    else:
        for name, token in DEPRECATIONS:
            # `present`, like the rows above. This site kept `in` when they were
            # converted -- the dimension was named and its sibling was not, which
            # is the second time in this round the same fix landed on one of two
            # places. A deprecation token is an identifier, so the same larger-
            # token false negative applies here.
            if not present(token, deprecated_body):
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

    # 10. ANOTHER VERSION -> FAIL, and this case was inverted once the consequence
    #     was measured. It used to assert SKIP with exit 0, on the reasoning that the
    #     4.2.0 engineer should not meet a red naming defects that are not theirs.
    #     True about the defects, and it made the floor DISARM ITSELF on the bump:
    #     4.2.0 ran this, was told the list describes 4.1.0, and passed having checked
    #     nothing -- on the release path, at the one moment the silence could not be
    #     recovered from. Two mages found it independently.
    #
    #     The red names no defect. It asks for a decision, and offers both answers.
    other = "## [4.2.0] - 2026-12-01" + "\n\n" + "### Added" + "\n\n" + "- An ordinary feature." + "\n"
    case("10 another version FAILS, not skips", 1, other, "A release needs its own list",
         manifest='version = "4.2.0"\n')

    # 11. The searched body is BUILT, not subtracted, and this asserts the property
    #     rather than a scenario -- because no scenario discriminates today. The old
    #     form was `section.replace(subs[DEPRECATED_HEADING], "")`, and an EMPTY
    #     `### Deprecated` has a body of one newline, so that removed every newline in
    #     the section. Every current token is a contiguous identifier and survived the
    #     collapse, which is exactly why the damage was invisible: the floor answered
    #     correctly for the wrong reason, and the first token to span a line break
    #     would have gone missing with the guard reporting OK.
    #
    #     So what is pinned is what the collapse destroyed: the LINE STRUCTURE of
    #     everything outside the excluded subsection. Concatenation preserves it by
    #     construction; subtraction did not.
    dep = "\n".join(["## [4.1.0] - 2026-09-05", "", "### Changed", "",
                     "- first line", "- second line", "",
                     DEPRECATED_HEADING, ""])
    parts = subsections(dep)
    rebuilt = "\n".join(b for h, b in parts.items() if h != DEPRECATED_HEADING)
    ok = "- first line\n- second line" in rebuilt
    print("  [%s] %-52s" % ("ok" if ok else "FAIL",
                            "11 an empty Deprecated does not collapse lines"))
    if not ok:
        failures.append("11: line structure lost, rebuilt=%r" % rebuilt)

    # 12. A token must not be satisfied by a LARGER one containing it. `in` was the
    #     first form, and the row whose token is `529` was then satisfied by any
    #     `5291` anywhere in the section -- a release could disclose nothing about
    #     that change and the floor would still report OK. The tokens are identifiers
    #     and numbers, which is the shape substring matching gets wrong.
    #
    #     Both directions, because a boundary applied blanket would break the tokens
    #     that end on punctuation: `529` inside `5291` must NOT count, and
    #     `exclude = [`, which finishes on a bracket, must still count.
    ok = (not present("529", "the suite grew to 5291 tests")
          and present("529", "HTTP 529 is now transient")
          and present("exclude = [", "the `exclude = [` list gained three entries"))
    print("  [%s] %-52s" % ("ok" if ok else "FAIL",
                            "12 a token cannot hide inside a larger one"))
    if not ok:
        failures.append("12: boundary matching wrong")

    # 13. ...and the deliberate escape works. A release with no silent change says so
    #     by name, which is a decision someone makes and a reviewer can see -- unlike
    #     the default skip it replaced, which nobody had to make and nobody could see.
    saved = dict(NO_DISCLOSURES)
    NO_DISCLOSURES["4.2.0"] = "docs only"
    try:
        case("13 a declared no-disclosure release passes", 0, other,
             "no silent change", manifest='version = "4.2.0"\n')
        # ...and an entry with NO REASON is refused. An escape that costs one word
        # is the cheapest way to make a red go away, and the cheapest escape is the
        # one a hurried person takes at release time.
        NO_DISCLOSURES["4.2.0"] = "   "
        case("13b a no-disclosure entry needs a reason", 1, other,
             "with no reason", manifest='version = "4.2.0"\n')
    finally:
        NO_DISCLOSURES.clear()
        NO_DISCLOSURES.update(saved)

    # 14. A DEPRECATION token cannot hide inside a larger one either. The rows above
    #     were converted to `present` and this site kept `in`, and reverting it left
    #     the whole self-test green -- the fix was unpinned, which is the shape this
    #     round keeps producing: the dimension is named and its sibling is not.
    #
    #     `majority_summary` mentioned only as `majority_summary_v2` is the case: a
    #     release could deprecate something else with a similar name and satisfy the
    #     row that exists to announce THIS one.
    hidden = _full_section().replace(
        "- `majority_summary` is deprecated and goes away in the next major.",
        "- `majority_summary_v2` is deprecated and goes away in the next major.")
    case("14 a deprecation cannot hide inside a larger token", 1, hidden,
         "missing `majority_summary`")

    # 15. The escape cannot be pointed at the version it is escaping FROM. With the
    #     release's own version in NO_DISCLOSURES the escape fired first and returned
    #     0 while every row and deprecation went unchecked -- sixteen findings on the
    #     same input without the entry. The escape was hardened once to cost a reason
    #     and stayed open on the flank nobody looked at.
    saved = dict(NO_DISCLOSURES)
    NO_DISCLOSURES[APPLIES_TO] = "nothing to see here"
    try:
        case("15 APPLIES_TO cannot also claim no disclosures", 1, _full_section(),
             "cannot also declare")
    finally:
        NO_DISCLOSURES.clear()
        NO_DISCLOSURES.update(saved)

    if failures:
        print("\nSELF-TEST FAILED:")
        for line in failures:
            print("  " + line)
        return 1
    print("\nSELF-TEST OK -- 20 cases")
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
