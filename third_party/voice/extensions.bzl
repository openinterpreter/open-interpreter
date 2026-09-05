"""Use standard Bazel archives for the standalone builder's pinned sources."""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_BUILD_FILE = """
filegroup(
    name = "sources",
    srcs = glob(["**"]),
    visibility = ["//visibility:public"],
)
"""

def _voice_sources_impl(module_ctx):
    manifest = json.decode(module_ctx.read(Label("//third_party/voice:sources.json")))
    for source in manifest["sources"]:
        http_archive(
            name = "voice_" + source["name"].replace("-", "_"),
            urls = [source["url"]],
            sha256 = source["sha256"],
            strip_prefix = source["root"],
            # Ninja's pinned codeload URL has no archive filename extension.
            type = "tar.gz" if source["name"] == "ninja" else "",
            build_file_content = _BUILD_FILE,
        )
    return module_ctx.extension_metadata(reproducible = True)

voice_sources = module_extension(implementation = _voice_sources_impl)
