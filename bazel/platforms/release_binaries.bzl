"""Rules for building release binaries across supported target platforms."""

load("@rules_platform//platform_data:defs.bzl", "platform_data")

_PLATFORM_TRIPLES = {
    "linux_arm64_musl": "aarch64-unknown-linux-musl",
    "linux_amd64_musl": "x86_64-unknown-linux-musl",
    "macos_amd64": "x86_64-apple-darwin",
    "macos_arm64": "aarch64-apple-darwin",
    "windows_amd64": "x86_64-pc-windows-msvc",
    "windows_arm64": "aarch64-pc-windows-msvc",
}

PLATFORMS = _PLATFORM_TRIPLES.keys()

def multiplatform_binaries(name, platforms = PLATFORMS):
    """Build a binary for a subset of the declared release platforms."""
    for platform in platforms:
        platform_data(
            name = name + "_" + platform,
            platform = "@rules_rs//rs/platforms:" + _PLATFORM_TRIPLES[platform],
            target = name,
            tags = ["manual"],
        )

    native.filegroup(
        name = "release_binaries",
        srcs = [name + "_" + platform for platform in platforms],
        tags = ["manual"],
    )
