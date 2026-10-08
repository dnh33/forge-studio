"""Tests for the update-signature verifier.

The fixtures are a throwaway keypair signed with the Tauri CLI, committed so the
verifier has something to be wrong about. The signature over `payload.bin` is
real minisign output, so these tests exercise the actual format rather than a
mock of it: prehashed BLAKE2b, the key id check, and the trusted-comment
binding.
"""
import importlib.util
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[1]
SCRIPT = REPO / "scripts" / "verify_update.py"
FIXTURES = Path(__file__).resolve().parent / "fixtures"


def load_module():
    spec = importlib.util.spec_from_file_location("verify_update", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules["verify_update"] = module  # dataclasses/annotations need this
    spec.loader.exec_module(module)
    return module


vu = load_module()


@pytest.fixture
def fixture_key():
    return vu.load_public_key(str(FIXTURES / "pubkey.txt"))


# ------------------------------------------------------------------ verifier

def test_the_fixture_signature_verifies(fixture_key):
    key_id, key = fixture_key
    ok, detail = vu.verify_one(
        FIXTURES / "payload.bin.sig", FIXTURES / "payload.bin", key_id, key
    )
    assert ok, detail
    assert "ED" in detail, "tauri signs prehashed; the verifier must handle that"


def test_a_tampered_payload_is_rejected(tmp_path, fixture_key):
    key_id, key = fixture_key
    shutil.copy(FIXTURES / "payload.bin.sig", tmp_path / "payload.bin.sig")
    (tmp_path / "payload.bin").write_bytes(
        (FIXTURES / "payload.bin").read_bytes() + b"one byte too many\n"
    )
    ok, detail = vu.verify_one(tmp_path / "payload.bin.sig", tmp_path / "payload.bin", key_id, key)
    assert not ok
    assert "does not match" in detail


def test_a_signature_from_another_key_is_named_as_such(fixture_key):
    """The real app key must not accept the fixture's signature."""
    app_key_id, app_key = vu.load_public_key()  # the key baked into the app
    fixture_key_id, _ = fixture_key
    assert app_key_id != fixture_key_id, "fixtures must use a different key than the app"
    ok, detail = vu.verify_one(
        FIXTURES / "payload.bin.sig", FIXTURES / "payload.bin", app_key_id, app_key
    )
    assert not ok
    assert "different key" in detail


def test_a_truncated_signature_file_is_rejected(tmp_path, fixture_key):
    key_id, key = fixture_key
    (tmp_path / "payload.bin").write_bytes((FIXTURES / "payload.bin").read_bytes())
    (tmp_path / "payload.bin.sig").write_text("dW50cnVzdGVkIGNvbW1lbnQ6IG9ubHkgb25lIGxpbmUK")
    ok, detail = vu.verify_one(tmp_path / "payload.bin.sig", tmp_path / "payload.bin", key_id, key)
    assert not ok
    assert "four lines" in detail


def test_a_garbage_public_key_is_refused():
    with pytest.raises(SystemExit):
        vu.load_public_key("bm90LWEta2V5")


# ----------------------------------------------------------------------- cli

def test_cli_verifies_the_fixture_directory():
    p = subprocess.run(
        [sys.executable, str(SCRIPT), str(FIXTURES), "--pubkey", str(FIXTURES / "pubkey.txt")],
        capture_output=True, text=True,
    )
    assert p.returncode == 0, p.stdout + p.stderr
    assert "1/1 signatures verified" in p.stdout


def test_cli_fails_and_exits_nonzero_on_a_tampered_copy(tmp_path):
    shutil.copy(FIXTURES / "payload.bin.sig", tmp_path / "payload.bin.sig")
    shutil.copy(FIXTURES / "payload.bin", tmp_path / "payload.bin")
    (tmp_path / "payload.bin").write_bytes(b"tampered\n")
    p = subprocess.run(
        [sys.executable, str(SCRIPT), str(tmp_path), "--pubkey", str(FIXTURES / "pubkey.txt")],
        capture_output=True, text=True,
    )
    assert p.returncode == 1, "a bad signature must fail the check"
    assert "0/1 signatures verified" in p.stdout


def test_cli_reports_a_signature_whose_payload_is_absent(tmp_path):
    shutil.copy(FIXTURES / "payload.bin.sig", tmp_path / "payload.bin.sig")
    p = subprocess.run(
        [sys.executable, str(SCRIPT), str(tmp_path), "--pubkey", str(FIXTURES / "pubkey.txt")],
        capture_output=True, text=True,
    )
    assert p.returncode == 1
    assert "MISSING" in p.stdout


def test_cli_says_so_when_there_is_nothing_to_check(tmp_path):
    p = subprocess.run(
        [sys.executable, str(SCRIPT), str(tmp_path)],
        capture_output=True, text=True,
    )
    assert p.returncode != 0
    assert "no *.sig files" in (p.stdout + p.stderr)


def test_the_app_key_loads_from_the_real_config():
    """A guard on the config: if the pubkey ever moves, this fails loudly."""
    key_id, key = vu.load_public_key()
    assert len(key_id) == 8
    assert key is not None
