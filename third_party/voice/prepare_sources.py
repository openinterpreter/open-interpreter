#!/usr/bin/env python3
"""Prepare pinned native voice sources offline; never execute archive contents."""

import argparse
from dataclasses import dataclass
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import sys
import tarfile

MANIFEST = Path(__file__).with_name("sources.json")
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024
MAX_SOURCE_BYTES = 512 * 1024 * 1024
MAX_MEMBERS = 100_000


@dataclass(frozen=True)
class Source:
    name: str
    version: str
    role: str
    archive: str
    root: str
    url: str
    sha256: str
    provenance: str


def load_sources(manifest: bytes) -> list[Source]:
    document = json.loads(manifest)
    if document["schema_version"] != 1:
        raise ValueError("Unsupported native source manifest version")
    sources = [Source(**entry) for entry in document["sources"]]
    seen = set()
    for source in sources:
        for field in (source.name, source.archive, source.root):
            if not re.fullmatch(r"[a-zA-Z0-9_-][a-zA-Z0-9_.-]*", field):
                raise ValueError(f"Invalid native source identifier: {field!r}")
        if not re.fullmatch(r"[0-9a-f]{64}", source.sha256):
            raise ValueError(f"Invalid SHA-256 for {source.name}")
        for kind, value in (
            ("name", source.name),
            ("archive", source.archive),
            ("root", source.root),
        ):
            key = (kind, value.casefold())
            if key in seen:
                raise ValueError(f"Duplicate native source {kind}: {value}")
            seen.add(key)
    if not sources:
        raise ValueError("Native source manifest is empty")
    return sources


def extract_source(source: Source, archives: Path, destination: Path) -> None:
    # Use one immutable byte snapshot for both verification and extraction.
    with (archives / source.archive).open("rb") as archive:
        data = archive.read(MAX_ARCHIVE_BYTES + 1)
    if len(data) > MAX_ARCHIVE_BYTES:
        raise ValueError(f"Archive exceeds size limit: {source.name}")
    if hashlib.sha256(data).hexdigest() != source.sha256:
        raise ValueError(f"SHA-256 mismatch: {source.name}")
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        members = []
        size = 0
        for member in archive:
            path = PurePosixPath(member.name)
            if path.parts[:1] != (source.root,) or ".." in path.parts:
                raise ValueError(f"Invalid archive path: {member.name!r}")
            size += member.size
            members.append(member)
            if size > MAX_SOURCE_BYTES or len(members) > MAX_MEMBERS:
                raise ValueError(f"Expanded source exceeds limits: {source.name}")
        if not members:
            raise ValueError(f"Empty source archive: {source.name}")
        # Bound Python's fallback copies when Windows cannot create archive links.
        links = sum(member.issym() or member.islnk() for member in members)
        if size + links * max(member.size for member in members) > MAX_SOURCE_BYTES:
            raise ValueError(f"Expanded source exceeds limits: {source.name}")
        archive.extractall(destination, members=members, filter="data")


def prepare_sources(archives: Path, output: Path, manifest: bytes) -> None:
    sources = load_sources(manifest)
    # Claim a new directory exclusively; never reuse or overwrite an earlier build.
    output.mkdir()
    try:
        for source in sources:
            extract_source(source, archives, output)
        receipt = {
            "schema_version": 1,
            "manifest_sha256": hashlib.sha256(manifest).hexdigest(),
            "sources": {
                source.name: {"root": source.root, "sha256": source.sha256}
                for source in sources
            },
        }
        (output / "sources.json").write_bytes(manifest)
        # This is the completion marker; failures never leave a valid receipt.
        (output / "prepared.json").write_text(
            json.dumps(receipt, indent=2) + "\n", encoding="utf-8"
        )
    except BaseException:
        shutil.rmtree(output)
        raise


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--archives",
        required=True,
        type=Path,
        help="Directory containing the pinned archives",
    )
    parser.add_argument(
        "--output", required=True, type=Path, help="New directory for verified sources"
    )
    args = parser.parse_args()
    if sys.version_info < (3, 12):
        parser.error("Native source preparation requires Python 3.12 or newer")
    prepare_sources(args.archives, args.output, MANIFEST.read_bytes())


if __name__ == "__main__":
    main()
