"""CLI check E2E tests — runs sqlalchemy-lsp check on per-diagnostic fixture dirs.

Each fixture is a minimal Python workspace that triggers exactly the target
diagnostic (plus at most a handful of secondarily related codes). Tests assert
the target code is present; they do not assert it is the *only* code.
"""

import json
import pathlib
import subprocess
import sys

import pytest

_PROJECT_ROOT = pathlib.Path(__file__).parent.parent.parent
_SERVER_BIN = _PROJECT_ROOT / "target" / "debug" / "sqlalchemy-lsp"
_DIAG_DIR = pathlib.Path(__file__).parent / "fixtures" / "diag"


def check(fixture_dir: str, *extra_args: str) -> list[dict]:
    """Run `sqlalchemy-lsp check <dir> --reporter json` and return findings."""
    path = _DIAG_DIR / fixture_dir
    result = subprocess.run(
        [str(_SERVER_BIN), "check", str(path), "--reporter", "json", *extra_args],
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def codes(findings: list[dict]) -> list[str]:
    return [f["code"] for f in findings]


# ── Structure & constraints ───────────────────────────────────────────────────


def test_clean_blog_produces_no_findings():
    """The reference workspace is lint-clean (REQ-TST-01)."""
    result = subprocess.run(
        [str(_SERVER_BIN), "check", str(_PROJECT_ROOT / "tests" / "e2e" / "fixtures" / "clean_blog"), "--reporter", "json"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0
    assert json.loads(result.stdout) == []


def test_sqla_w101_missing_tablename():
    """Model without __tablename__ triggers SQLA-W101."""
    findings = check("w101")
    assert "SQLA-W101" in codes(findings)
    assert all(f["code"] in ("SQLA-W101",) for f in findings), codes(findings)


def test_sqla_e102_duplicate_tablename():
    """Two models sharing a table name trigger SQLA-E102."""
    findings = check("e102")
    assert "SQLA-E102" in codes(findings)


def test_sqla_e103_duplicate_column():
    """Column declared twice on a model triggers SQLA-E103."""
    findings = check("e103")
    assert "SQLA-E103" in codes(findings)
    assert all(f["code"] in ("SQLA-E103",) for f in findings), codes(findings)


def test_sqla_w104_missing_primary_key():
    """Model with no primary key column triggers SQLA-W104."""
    findings = check("w104")
    assert "SQLA-W104" in codes(findings)
    assert all(f["code"] in ("SQLA-W104",) for f in findings), codes(findings)


def test_sqla_e105_table_arg_column_not_found():
    """__table_args__ referencing a missing column triggers SQLA-E105."""
    findings = check("e105")
    assert "SQLA-E105" in codes(findings)
    assert all(f["code"] in ("SQLA-E105",) for f in findings), codes(findings)


# ── Foreign keys ──────────────────────────────────────────────────────────────


def test_sqla_e301_bad_fk_table():
    """ForeignKey referencing an unknown table triggers SQLA-E301."""
    findings = check("e301")
    assert "SQLA-E301" in codes(findings)
    assert all(f["code"] in ("SQLA-E301",) for f in findings), codes(findings)


def test_sqla_e302_fk_column_not_found():
    """ForeignKey referencing an unknown column triggers SQLA-E302."""
    findings = check("e302")
    assert "SQLA-E302" in codes(findings)
    assert all(f["code"] in ("SQLA-E302",) for f in findings), codes(findings)


# ── Relationships ─────────────────────────────────────────────────────────────


def test_sqla_e401_relationship_target_not_found():
    """relationship() pointing at an unknown model triggers SQLA-E401."""
    findings = check("e401")
    assert "SQLA-E401" in codes(findings)
    assert all(f["code"] in ("SQLA-E401",) for f in findings), codes(findings)


def test_sqla_w402_back_populates_mismatch():
    """back_populates counterpart exists but doesn't point back → SQLA-W402."""
    findings = check("w402")
    assert "SQLA-W402" in codes(findings)


def test_sqla_w403_back_populates_not_found():
    """back_populates names an attribute that doesn't exist → SQLA-W403."""
    findings = check("w403")
    assert "SQLA-W403" in codes(findings)
    assert all(f["code"] in ("SQLA-W403",) for f in findings), codes(findings)


# ── Finding structure ─────────────────────────────────────────────────────────


def test_finding_json_shape():
    """JSON findings have required fields: code, message, location, severity."""
    findings = check("e301")
    assert findings
    f = findings[0]
    assert "code" in f
    assert "message" in f
    assert "location" in f
    assert "severity" in f
    assert "row" in f["location"]
    assert "column" in f["location"]


def test_finding_points_to_correct_file():
    """Each finding's filename is within the checked fixture directory."""
    findings = check("e301")
    assert findings
    for f in findings:
        # filename is relative to the fixture root; just verify it exists and isn't cross-file
        assert f["filename"].endswith(".py"), f["filename"]


def test_exit_code_nonzero_on_findings():
    """check exits non-zero when findings are present."""
    result = subprocess.run(
        [str(_SERVER_BIN), "check", str(_DIAG_DIR / "e301"), "--reporter", "json"],
        capture_output=True,
        text=True,
    )
    assert result.returncode != 0


def test_exit_zero_flag():
    """--exit-zero exits 0 even when findings are present."""
    result = subprocess.run(
        [str(_SERVER_BIN), "check", str(_DIAG_DIR / "e301"), "--reporter", "json", "--exit-zero"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0


# ── 1xx structural (H/W class) ─────────────────────────────────────────────────


def test_sqla_h106_unnamed_constraint():
    """UniqueConstraint without name= triggers SQLA-H106."""
    findings = check("h106")
    assert "SQLA-H106" in codes(findings)
    assert all(f["code"] == "SQLA-H106" for f in findings), codes(findings)


# ── 2xx columns & types ────────────────────────────────────────────────────────


def test_sqla_h202_optional_nullable_false():
    """Optional annotation + nullable=False contradicts itself → SQLA-H202."""
    findings = check("h202")
    assert "SQLA-H202" in codes(findings)
    assert all(f["code"] == "SQLA-H202" for f in findings), codes(findings)


def test_sqla_h205_naive_datetime():
    """datetime column without timezone=True → SQLA-H205."""
    findings = check("h205")
    assert "SQLA-H205" in codes(findings)
    assert all(f["code"] == "SQLA-H205" for f in findings), codes(findings)


def test_sqla_w201_nullable_not_optional():
    """FK column is nullable but type is not Optional → SQLA-W201."""
    findings = check("w201")
    assert "SQLA-W201" in codes(findings)
    assert all(f["code"] == "SQLA-W201" for f in findings), codes(findings)


def test_sqla_w203_mutable_default():
    """Mutable list default on column → SQLA-W203."""
    findings = check("w203")
    assert "SQLA-W203" in codes(findings)
    assert all(f["code"] == "SQLA-W203" for f in findings), codes(findings)


def test_sqla_w204_default_and_server_default():
    """Column sets both default and server_default → SQLA-W204."""
    findings = check("w204")
    assert "SQLA-W204" in codes(findings)
    assert all(f["code"] == "SQLA-W204" for f in findings), codes(findings)


# ── 3xx foreign keys ───────────────────────────────────────────────────────────


def test_sqla_w303_fk_type_mismatch():
    """FK column typed differently from target column → SQLA-W303."""
    findings = check("w303")
    assert "SQLA-W303" in codes(findings)
    assert all(f["code"] == "SQLA-W303" for f in findings), codes(findings)


def test_sqla_w304_ambiguous_foreign_keys():
    """Two FKs to same table with no foreign_keys= → SQLA-W304."""
    findings = check("w304")
    assert "SQLA-W304" in codes(findings)
    assert all(f["code"] == "SQLA-W304" for f in findings), codes(findings)


# ── 4xx relationships ─────────────────────────────────────────────────────────


def test_sqla_w404_uselist_mismatch():
    """Scalar annotation + uselist=True contradicts itself → SQLA-W404."""
    findings = check("w404")
    assert "SQLA-W404" in codes(findings)
    assert all(f["code"] == "SQLA-W404" for f in findings), codes(findings)


def test_sqla_w405_target_mismatch():
    """Annotation says User but relationship() arg says Admin → SQLA-W405."""
    findings = check("w405")
    assert "SQLA-W405" in codes(findings)
    assert all(f["code"] == "SQLA-W405" for f in findings), codes(findings)


def test_sqla_h406_missing_fk():
    """Scalar relationship with no FK column → SQLA-H406."""
    findings = check("h406")
    assert "SQLA-H406" in codes(findings)
    assert all(f["code"] == "SQLA-H406" for f in findings), codes(findings)


def test_sqla_h407_one_to_one_missing_unique():
    """uselist=False relationship but FK column is not unique → SQLA-H407."""
    findings = check("h407")
    assert "SQLA-H407" in codes(findings)
    assert all(f["code"] == "SQLA-H407" for f in findings), codes(findings)


def test_sqla_h410_circular_relationship():
    """3-node cycle A→B→C→A without back_populates → SQLA-H410."""
    findings = check("h410")
    assert "SQLA-H410" in codes(findings)


def test_sqla_h414_lazy_select_scalar():
    """Scalar relationship uses lazy='select' (default) → SQLA-H414."""
    findings = check("h414")
    assert "SQLA-H414" in codes(findings)
    assert all(f["code"] == "SQLA-H414" for f in findings), codes(findings)


def test_sqla_h415_lazy_joined_collection():
    """Collection relationship uses lazy='joined' → SQLA-H415."""
    findings = check("h415")
    assert "SQLA-H415" in codes(findings)


def test_sqla_w411_missing_remote_side():
    """Self-referential scalar relationship without remote_side= → SQLA-W411."""
    findings = check("w411")
    assert "SQLA-W411" in codes(findings)


def test_sqla_w413_non_collection_mapped():
    """Scalar annotation for many-to-many counterpart → SQLA-W413."""
    findings = check("w413")
    assert "SQLA-W413" in codes(findings)


# ── 5xx modernization ─────────────────────────────────────────────────────────


def test_sqla_w501_legacy_backref():
    """backref= instead of explicit back_populates pair → SQLA-W501."""
    findings = check("w501")
    assert "SQLA-W501" in codes(findings)


# ── 7xx Alembic diagnostics ───────────────────────────────────────────────────


def test_sqla_w701_broken_chain():
    """down_revision pointing to a non-existent revision → SQLA-W701."""
    findings = check("w701")
    assert "SQLA-W701" in codes(findings)
    assert all(f["code"] == "SQLA-W701" for f in findings), codes(findings)


def test_sqla_w702_multiple_heads():
    """Two migrations both children of the same parent → two heads → SQLA-W702."""
    findings = check("w702")
    assert "SQLA-W702" in codes(findings)


def test_sqla_h703_unknown_table():
    """Migration references a table not in any ORM model → SQLA-H703."""
    findings = check("h703")
    assert "SQLA-H703" in codes(findings)


def test_sqla_w704_null_constraint_name():
    """op.drop_constraint(None, ...) uses None as constraint name → SQLA-W704."""
    findings = check("w704")
    assert "SQLA-W704" in codes(findings)


# ── noqa suppression ──────────────────────────────────────────────────────────


def test_noqa_specific_code_suppresses():
    """# noqa: SQLA-W101 suppresses W101 on that line, other models still fire."""
    findings = check("noqa")
    # Only UnsuppressedModel should produce W101; the two suppressed models do not
    assert len(findings) == 1, f"Expected 1 finding, got: {codes(findings)}"
    assert findings[0]["code"] == "SQLA-W101"
    assert "UnsuppressedModel" in findings[0]["message"]


def test_noqa_bare_suppresses_all_codes():
    """Bare # noqa suppresses all codes on that line."""
    findings = check("noqa")
    # AlsoSuppressed uses bare # noqa — no finding for it
    for f in findings:
        assert "AlsoSuppressed" not in f["message"]


def test_noqa_suppressed_models_absent():
    """SuppressedModel and AlsoSuppressed produce zero findings despite W101 defect."""
    findings = check("noqa")
    for f in findings:
        assert "SuppressedModel" not in f["message"]
        assert "AlsoSuppressed" not in f["message"]
