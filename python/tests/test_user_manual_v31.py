#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Doc contract for the v3.1 migration block in ``USER_MANUAL.md``.

A v3.1.0 upgrade is only safe if the manual states all four migration facts that the
reviewer flagged (C24):

  (a) volume went ``u64`` -> ``f64`` in the FlatBuffer schema and in ``DataHandlerVTable``;
  (b) the loader requires the schema ``file_identifier`` and rejects legacy files with a
      migration error (instead of misreading the volumes);
  (c) every market-data file must be regenerated with the fixed ``csv-to-flatbuffer``;
  (d) every strategy library must be rebuilt against the current ``farukon_core``.

The assertions are keyword-based but they run against the v3.1 breaking-change block only,
so deleting or gutting that block fails the test. Standard library only; runs directly or
under pytest.

Run directly:  python python/tests/test_user_manual_v31.py
Under pytest:  pytest python/tests/test_user_manual_v31.py
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
MANUAL = os.path.join(REPO_ROOT, "USER_MANUAL.md")

SECTION_MARKER = "breaking change (v3.1.0)"

FACTS = {
    "volume type u64 -> f64 in the FlatBuffer schema and DataHandlerVTable": (
        "volume", "u64", "f64", "flatbuffer", "datahandlervtable",
    ),
    "file_identifier guard rejects legacy files with a migration error": (
        "file_identifier", "reject", "migration error",
    ),
    "market data must be regenerated with csv-to-flatbuffer": (
        "regenerate every market-data file", "csv-to-flatbuffer",
    ),
    "every strategy must be rebuilt against farukon_core": (
        "rebuild every strategy library", "farukon_core",
    ),
}


def _manual_text():
    with open(MANUAL, "r", encoding="utf-8") as fin:
        return fin.read()


def _normalize(text):
    """Lowercase, drop blockquote markers, collapse all whitespace runs to one space."""
    lines = []
    for line in text.splitlines():
        stripped = line.lstrip()
        while stripped.startswith(">"):
            stripped = stripped[1:].lstrip()
        lines.append(stripped)
    return re.sub(r"\s+", " ", " ".join(lines)).strip().lower()


def _v31_section(text=None):
    """The v3.1.0 breaking-change block of the manual, normalized.

    Starts at the ``> **Breaking change (v3.1.0) ...`` block and ends at the next Markdown
    heading (``### 8.1 ...``), i.e. the block a reader upgrading from v3.0.0 sees first.
    """
    raw = _manual_text() if text is None else text
    lines = raw.splitlines()
    start = None
    for index, line in enumerate(lines):
        if SECTION_MARKER in line.lower():
            start = index
            break
    assert start is not None, (
        "USER_MANUAL.md has no v3.1.0 breaking-change block (searched for %r)"
        % (SECTION_MARKER,)
    )

    block = []
    for line in lines[start + 1:]:
        if line.lstrip().startswith("#"):
            break
        block.append(line)
    return _normalize("\n".join(block))


def test_manual_exists_at_repo_root():
    assert os.path.isfile(MANUAL), "USER_MANUAL.md is missing at the repo root: %s" % (
        MANUAL,)


def test_v31_migration_block_is_present_and_substantial():
    section = _v31_section()
    assert len(section.split()) >= 60, (
        "the v3.1.0 breaking-change block looks gutted (%d words)"
        % (len(section.split()),)
    )


def _assert_fact(label, keywords, section=None):
    text = _v31_section() if section is None else section
    missing = [keyword for keyword in keywords if keyword not in text]
    assert not missing, (
        "USER_MANUAL.md no longer documents %s; missing keywords %r"
        % (label, missing)
    )


def test_volume_is_documented_as_f64_in_schema_and_vtable():
    """(a) volume u64 -> f64 in the FlatBuffer schema / DataHandlerVTable."""
    _assert_fact(
        "volume u64 -> f64 in the FlatBuffer schema / DataHandlerVTable",
        FACTS["volume type u64 -> f64 in the FlatBuffer schema and DataHandlerVTable"],
    )


def test_file_identifier_guard_is_documented():
    """(b) the file_identifier guard rejects legacy files with a migration error."""
    _assert_fact(
        "the file_identifier guard rejecting legacy files",
        FACTS["file_identifier guard rejects legacy files with a migration error"],
    )


def test_market_data_regeneration_is_documented():
    """(c) market data must be regenerated with the fixed csv-to-flatbuffer."""
    _assert_fact(
        "market-data regeneration with csv-to-flatbuffer",
        FACTS["market data must be regenerated with csv-to-flatbuffer"],
    )


def test_strategy_rebuild_is_documented():
    """(d) every strategy library must be rebuilt."""
    _assert_fact(
        "the mandatory strategy rebuild",
        FACTS["every strategy must be rebuilt against farukon_core"],
    )


def test_all_four_migration_facts_share_the_v31_block():
    section = _v31_section()
    for label, keywords in FACTS.items():
        missing = [keyword for keyword in keywords if keyword not in section]
        assert not missing, "%r is not stated in the v3.1.0 block (missing %r)" % (
            label, missing)


def _run_all():
    failures = 0
    for name in sorted(globals()):
        func = globals()[name]
        if name.startswith("test_") and callable(func):
            try:
                func()
                print("PASS %s" % name)
            except Exception as exc:  # noqa: BLE001 - test runner
                failures += 1
                print("FAIL %s -> %r" % (name, exc))
    print("%d failed" % failures)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run_all())
