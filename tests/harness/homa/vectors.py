# SPDX-License-Identifier: MIT
"""Fetches the published vectors of the Homa track from pinned URLs.

No vector of the track is typed by hand. This file fetches each text from its
pinned URL, checks the SHA-256 the text had when it was read, parses the
vectors out of it and writes them under `tests/capture/homa_vectors/`, where
the notebooks read them. A text whose SHA-256 moved stops the write, so a
changed source is seen before it changes a vector.

    tests/.venv/bin/python -m harness.homa.vectors

run from `tests/`, or with `tests/` on `PYTHONPATH`, writes `crc32c.json`
from RFC 3720 appendix B.4 and the check value of the CRC catalogue's entry
CRC-32/ISCSI, and since stage H6 `chacha20poly1305.json` from the two vectors
of AEAD_CHACHA20_POLY1305 of RFC 8439, section 2.8.2 and appendix A.5.
"""

import hashlib
import json
import re
import sys
import urllib.request
from pathlib import Path

RFC3720 = {
    "url": "https://www.rfc-editor.org/rfc/rfc3720.txt",
    "sha256": "0c014dbc041bfc2308c1990387aabffdb21050c3ec140c96522c12005b572db3",
    "read": "2026-09-27",
    "section": "appendix B.4, CRC Examples",
}
CATALOGUE = {
    "url": "https://reveng.sourceforge.io/crc-catalogue/17plus.htm",
    "sha256": "dee3b75edd21185ba276450f8ae29f213d8f740207b74652967f6b44e55aefbb",
    "read": "2026-09-27",
    "section": "CRC-32/ISCSI",
}
RFC8439 = {
    "url": "https://www.rfc-editor.org/rfc/rfc8439.txt",
    "sha256": "25bef70fbf7a07ff45c2fe4cb7c6ce954eac687413d8610603268b4e4415324c",
    "read": "2026-09-28",
    "section": "2.8.2, Example and Test Vector for AEAD_CHACHA20_POLY1305, and A.5, ChaCha20-Poly1305 AEAD Decryption",
}


class SourceMoved(RuntimeError):
    """A pinned text no longer has the SHA-256 it had when it was read."""


def fetch(source: dict) -> str:
    """Return the text at `source["url"]` once its SHA-256 matches the pin."""
    request = urllib.request.Request(source["url"], headers={"User-Agent": "aosabi-harness"})
    with urllib.request.urlopen(request, timeout=60) as response:
        body = response.read()
    digest = hashlib.sha256(body).hexdigest()
    if digest != source["sha256"]:
        raise SourceMoved("%s has SHA-256 %s, the pin is %s" % (source["url"], digest, source["sha256"]))
    return body.decode("utf-8", errors="replace")


def crc_examples(rfc: str) -> list:
    """Return the vectors of RFC 3720 appendix B.4 as data and CRC bytes.

    The appendix prints each example as rows of four bytes with their
    offsets and elides the rows between the first and the last of the four
    runs of 32 bytes with `...`. Every elided byte continues the step its
    column takes from the first row to the last, which reproduces the zeroes,
    the ones and the incrementing and decrementing runs the titles name, and
    the notebook checks the result against those titles.
    """
    start = rfc.index("B.4.  CRC Examples")
    end = rfc.index("Appendix C.", start)
    text = rfc[start:end]
    examples = []
    title = None
    rows = []
    elided = False
    for line in text.splitlines():
        stripped = line.strip()
        row = re.match(r"^(\d+):\s+((?:[0-9a-f]{2}\s*){4})$", stripped)
        crc = re.match(r"^CRC:\s+((?:[0-9a-f]{2}\s*){4})$", stripped)
        if crc:
            data = _expand(rows, elided)
            examples.append(
                {
                    "name": title,
                    "data": data.hex(),
                    "crc": bytes.fromhex(crc.group(1)).hex(),
                }
            )
            rows, elided, title = [], False, None
        elif row:
            rows.append((int(row.group(1)), bytes.fromhex(row.group(2))))
        elif stripped == "...":
            elided = True
        elif title is None and _is_title(stripped):
            title = stripped.rstrip(":")
    return examples


def _is_title(line: str) -> bool:
    """Tell whether a line of the appendix names the example that follows.

    Every other line is blank, the heading, the note on hexadecimal, the
    header of a byte table or a running header or footer of a page.
    """
    return bool(line) and not (
        line.startswith(("B.4.", "N.B.", "Byte:", "RFC 3720", "Satran"))
    )


def _expand(rows: list, elided: bool) -> bytes:
    """Return the bytes of rows of four, filling an elided run linearly."""
    if not elided:
        return b"".join(data for _, data in rows)
    (first_offset, first), (last_offset, last) = rows[0], rows[-1]
    count = (last_offset - first_offset) // 4
    out = bytearray()
    for step in range(count + 1):
        for column in range(4):
            delta = last[column] - first[column]
            out.append(first[column] + delta * step // count)
    return bytes(out)


def check_value(catalogue: str) -> dict:
    """Return the parameters and the check value of CRC-32/ISCSI."""
    line = re.search(r'<CODE>(width=32 [^<]*name="CRC-32/ISCSI")</CODE>', catalogue).group(1)
    fields = dict(re.findall(r'(\w+)=("[^"]*"|\S+)', line))
    return {
        "name": fields["name"].strip('"'),
        "poly": fields["poly"],
        "init": fields["init"],
        "refin": fields["refin"],
        "refout": fields["refout"],
        "xorout": fields["xorout"],
        "check": fields["check"],
        "check_text": "123456789",
    }


def write_crc32c(out_dir: Path) -> Path:
    """Fetch both sources and write `crc32c.json`."""
    document = {
        "sources": [RFC3720, CATALOGUE],
        "vectors": crc_examples(fetch(RFC3720)),
        "check": check_value(fetch(CATALOGUE)),
    }
    path = out_dir / "crc32c.json"
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    return path


_DUMP = re.compile(r"^\s*\d{3}  ((?:[0-9a-f]{2} )*[0-9a-f]{2})(?:  |\s*$)")
_COLONS = re.compile(r"^\s*((?:[0-9a-f]{2}:)+[0-9a-f]{2}):?\s*$")


def _section(rfc: str, heading: str, after: str) -> str:
    """Return the body of the section `heading` up to the heading `after`.

    The table of contents names each heading as well, followed by dots and a
    page, so the body starts at the heading that stands on a line of its own.
    """
    start = rfc.index("\n" + heading + "\n") + 1
    return rfc[start:rfc.index("\n" + after, start)]


def _labelled(text: str, label: str) -> bytes:
    """Return the bytes printed under the line `label` of a section.

    The RFC prints a value as rows of an offset and up to 16 bytes, followed
    by their characters, or as bytes joined by colons, and ends it with a
    blank line.
    """
    lines = text.splitlines()
    at = next(i for i, line in enumerate(lines) if line.strip() == label)
    out = bytearray()
    for line in lines[at + 1:]:
        dump = _DUMP.match(line)
        colons = _COLONS.match(line)
        if dump:
            out += bytes.fromhex(dump.group(1))
        elif colons:
            out += bytes.fromhex(colons.group(1).replace(":", ""))
        elif line.strip() or out:
            break
    return bytes(out)


def aead_vectors(rfc: str) -> list:
    """Return the two vectors of AEAD_CHACHA20_POLY1305 of RFC 8439.

    Section 2.8.2 prints the nonce as the 32 bit common part and the IV of 8
    bytes, which section 2.8 concatenates in that order, and appendix A.5
    prints the ciphertext and the tag it receives and the plaintext the
    decryption yields.
    """
    encrypt = _section(
        rfc,
        "2.8.2.  Example and Test Vector for AEAD_CHACHA20_POLY1305",
        "3.  Implementation Advice",
    )
    decrypt = _section(rfc, "A.5.  ChaCha20-Poly1305 AEAD Decryption", "Appendix B.")
    return [
        {
            "name": "RFC 8439 section 2.8.2",
            "key": _labelled(encrypt, "Key:").hex(),
            "nonce": (_labelled(encrypt, "32-bit fixed-common part:") + _labelled(encrypt, "IV:")).hex(),
            "aad": _labelled(encrypt, "AAD:").hex(),
            "plaintext": _labelled(encrypt, "Plaintext:").hex(),
            "ciphertext": _labelled(encrypt, "Ciphertext:").hex(),
            "tag": _labelled(encrypt, "Tag:").hex(),
        },
        {
            "name": "RFC 8439 appendix A.5",
            "key": _labelled(decrypt, "The ChaCha20 Key").hex(),
            "nonce": _labelled(decrypt, "The nonce:").hex(),
            "aad": _labelled(decrypt, "The AAD:").hex(),
            "plaintext": _labelled(decrypt, "Plaintext::").hex(),
            "ciphertext": _labelled(decrypt, "Ciphertext:").hex(),
            "tag": _labelled(decrypt, "Received Tag:").hex(),
        },
    ]


def write_chacha20poly1305(out_dir: Path) -> Path:
    """Fetch RFC 8439 and write `chacha20poly1305.json`."""
    document = {"sources": [RFC8439], "vectors": aead_vectors(fetch(RFC8439))}
    path = out_dir / "chacha20poly1305.json"
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    return path


def main() -> int:
    out_dir = Path(__file__).resolve().parents[2] / "capture" / "homa_vectors"
    out_dir.mkdir(parents=True, exist_ok=True)
    print(write_crc32c(out_dir))
    print(write_chacha20poly1305(out_dir))
    return 0


if __name__ == "__main__":
    sys.exit(main())
