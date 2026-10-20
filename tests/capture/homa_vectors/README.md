# The published vectors of the Homa track

This directory holds the vectors the Homa track's notebooks judge against, each parsed out of a
published text that `tests/harness/homa/vectors.py` fetches from a pinned URL. The script checks the
SHA-256 each text had when it was read and stops before it writes a file when a text moved, so no
vector is typed by hand and a changed source is seen before it changes a vector.

The files are written by the script, from `tests/`:

```
.venv/bin/python -m harness.homa.vectors
```

## What each file holds

| File | Holds | Sources | Read by |
| --- | --- | --- | --- |
| `crc32c.json` | The five examples of RFC 3720 appendix B.4, each its data and the four bytes of its CRC-32C in the order they are appended, and the parameters and the check value of the entry CRC-32/ISCSI of the CRC catalogue, 0xe3069283 over the text 123456789 | https://www.rfc-editor.org/rfc/rfc3720.txt with SHA-256 `0c014dbc041bfc2308c1990387aabffdb21050c3ec140c96522c12005b572db3`, and https://reveng.sourceforge.io/crc-catalogue/17plus.htm with SHA-256 `dee3b75edd21185ba276450f8ae29f213d8f740207b74652967f6b44e55aefbb`, both read | `tests/notebooks/04_aoframe.ipynb` |
| `chacha20poly1305.json` | The two vectors of AEAD_CHACHA20_POLY1305 of RFC 8439, section 2.8.2, which encrypts, and appendix A.5, which decrypts, each its key, its nonce, its associated data, its plaintext, its ciphertext and its tag | https://www.rfc-editor.org/rfc/rfc8439.txt with SHA-256 `25bef70fbf7a07ff45c2fe4cb7c6ce954eac687413d8610603268b4e4415324c`, read | `tests/notebooks/06_lora_frame.ipynb` |

The appendix prints the four runs of 32 bytes as their first and last rows of four bytes with the
rows between them elided, and the script fills each elided byte with the step its column takes
from the first row to the last. That reproduces the zeroes, the ones and the incrementing and
decrementing runs the titles of the examples name, which `04_aoframe` checks against the titles
before it uses a vector.

Section 2.8.2 prints the nonce of its vector as the 32 bit common part and the IV of 8 bytes, which
section 2.8 of the RFC joins in that order, and the script writes the nonce of 12 bytes so joined.
Every other value is the rows the RFC prints under its label, as a dump of offsets and bytes or as
bytes joined by colons.
