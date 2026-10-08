#!/usr/bin/env python3
"""Verify a release's updater signatures against the key baked into the app.

The Tauri updater refuses any artifact whose minisign signature does not match
the public key in tauri.conf.json. That is the security of the update path, so it
should be provable without publishing a release and installing the app.

This reads the public key out of the config, then verifies every `*.sig` it finds
against the file it names. It works on a downloaded release, and it works in the
release workflow on the freshly built bundles.

    python3 scripts/verify_update.py <dir-or-file> [more...]

Exit code 0 means every signature verified.
"""
import base64
import json
import sys
from hashlib import blake2b
from pathlib import Path

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

ROOT = Path(__file__).resolve().parents[1]
CONF = ROOT / "app" / "src-tauri" / "tauri.conf.json"


def load_public_key():
    """Return (key_id, Ed25519PublicKey) from the pubkey in tauri.conf.json.

    Tauri stores the base64 of an entire minisign public key *file* there, so it
    is decoded once to text and once again to the binary key.
    """
    conf = json.loads(CONF.read_text(encoding="utf-8"))
    stored = conf["plugins"]["updater"]["pubkey"]
    text = base64.b64decode(stored).decode("utf-8")
    blob = base64.b64decode(text.strip().splitlines()[-1])
    algorithm, key_id, raw = blob[:2], blob[2:10], blob[10:]
    if algorithm != b"Ed" or len(raw) != 32:
        raise SystemExit(f"unexpected public key format: {algorithm!r}, {len(raw)} bytes")
    return key_id, Ed25519PublicKey.from_public_bytes(raw)


def verify_one(sig_path: Path, payload_path: Path, key_id, key):
    """Verify one signature file against its artifact. Returns (ok, detail)."""
    text = base64.b64decode(sig_path.read_bytes().strip()).decode("utf-8")
    lines = [l.rstrip("\r") for l in text.splitlines() if l.strip()]
    if len(lines) < 4:
        return False, "signature file has fewer than four lines"

    comment, sig_line, trusted_line, global_line = lines[0], lines[1], lines[2], lines[3]
    blob = base64.b64decode(sig_line)
    algorithm, sig_key_id, signature = blob[:2], blob[2:10], blob[10:]

    if sig_key_id != key_id:
        return False, (
            "signed by a different key (id "
            + sig_key_id.hex()
            + "), not the one baked into the app ("
            + key_id.hex()
            + ")"
        )

    data = payload_path.read_bytes()
    # "ED" is minisign's prehashed mode: the signature covers BLAKE2b-512 of the
    # file rather than the file itself. Tauri uses it for anything sizeable.
    if algorithm == b"ED":
        data = blake2b(data, digest_size=64).digest()
    elif algorithm != b"Ed":
        return False, f"unknown algorithm {algorithm!r}"

    try:
        key.verify(signature, data)
    except InvalidSignature:
        return False, "the signature does not match the file contents"

    # The trusted comment is separately signed, binding it to this signature.
    # minisign signs the raw 64-byte signature concatenated with the comment
    # text, not the full algorithm+keyid+signature blob.
    if not trusted_line.startswith("trusted comment: "):
        return False, "missing trusted comment"
    trusted_comment = trusted_line[len("trusted comment: "):].encode()
    global_sig = base64.b64decode(global_line)
    try:
        key.verify(global_sig, signature + trusted_comment)
    except InvalidSignature:
        return False, "the trusted comment is not bound to this signature"

    stamp = trusted_comment.decode("utf-8", "replace").split("\t")[0]
    return True, f"{sig_path.name}  ({stamp}, {algorithm.decode()})"


def collect(targets):
    files = []
    for target in targets:
        p = Path(target)
        if p.is_dir():
            files.extend(sorted(p.rglob("*.sig")))
        elif p.suffix == ".sig" and p.is_file():
            files.append(p)
    return files


def main():
    targets = sys.argv[1:]
    if not targets:
        raise SystemExit(__doc__.strip().splitlines()[-2].strip())

    key_id, key = load_public_key()
    print(f"checking against the key baked into the app ({key_id.hex()})")

    sigs = collect(targets)
    if not sigs:
        raise SystemExit(f"no *.sig files found under: {' '.join(targets)}")

    failed = 0
    for sig_path in sigs:
        payload_path = sig_path.with_suffix("")  # strip .sig
        if not payload_path.is_file():
            print(f"  MISSING   {payload_path.name} (the file this signature names)")
            failed += 1
            continue
        # A .sig download may be named <artifact>.sig where the artifact has
        # further suffix stripped differently; fall back to a name match.
        ok, detail = verify_one(sig_path, payload_path, key_id, key)
        print(("  ok        " if ok else "  FAILED    ") + detail)
        failed += 0 if ok else 1

    print(f"{len(sigs) - failed}/{len(sigs)} signatures verified")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
