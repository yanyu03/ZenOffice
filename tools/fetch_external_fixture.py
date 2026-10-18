"""Opt-in download of one pinned external interoperability input; never overwrite."""
import hashlib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
URL = 'https://siu.edu/admissions/graduate/_common/documents/thesis-dissertation-researchpaper/thesis-template.docx'
EXPECTED = 'd47afcac2aa8e9c4d531fd5deb23ed33767d4fad64c8bf8649ec8c7b0b77d275'
DEST = ROOT / 'RCD/red-team-corpus/academic-word-2026-10/siu_thesis_template_original.docx'


def main():
    if DEST.exists():
        if hashlib.sha256(DEST.read_bytes()).hexdigest() != EXPECTED:
            raise SystemExit('Existing input differs; refusing overwrite')
        print('Pinned external fixture already present and verified')
        return
    with urllib.request.urlopen(URL, timeout=30) as response:
        data = response.read(2 * 1024 * 1024 + 1)
    if len(data) > 2 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != EXPECTED:
        raise SystemExit('Upstream input differs from frozen baseline; no file written')
    # Exclusive creation prevents another process from replacing an input unnoticed.
    with DEST.open('xb') as output:
        output.write(data)
    print('Downloaded pinned external fixture; excluded from Git')


if __name__ == '__main__':
    main()
