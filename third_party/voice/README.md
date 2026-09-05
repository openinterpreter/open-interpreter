# Native voice source inputs

This stage pins and prepares sources for a privately bundled, GStreamer-based
audio runtime, including its native dependencies and build tools. It does not
compile native libraries, link them into Codex or enable voice.

`sources.json` records the versions, URLs and SHA-256 digests of 11 archives:

| Purpose | Sources |
| --- | --- |
| GStreamer framework and plugin sources | `gstreamer`, `gst-plugins-base`, `gst-plugins-good` |
| Supporting native libraries | `glib`, `libffi`, `pcre2`, `zlib`, `proxy-libintl` |
| Audio codec | `opus` |
| Build tools, not runtime libraries | `meson`, `ninja` |

GLib also includes `gvdb` in its archive; it is recorded without a separate fetch.
These inputs do not include the complete platform toolchain for native builds.

Bazel uses standard `http_archive` rules to fetch, verify, unpack and cache the
archives from that manifest. Run from the repository root:

```sh
bazel build //third_party/voice:sources
```

For offline preparation without Bazel, use Python 3.12 or newer and an existing
archive directory whose filenames match the manifest:

```sh
python3 third_party/voice/prepare_sources.py --archives /path/to/archives --output /path/to/new-sources
python3 -m unittest discover -s third_party/voice -p 'test_*.py'
```

The adapter verifies archive digests and bounds, then extracts with Python's
`tarfile` data filter. It preserves links where supported and copies their archive
targets when link creation is unavailable, including on Windows. It refuses an
existing output directory and cleans up incomplete output. `prepared.json` records
successful preparation, not the integrity of later edits to the extracted tree.

Ordinary CLI builds do not run either path. The Bazel `:sources` target is
`manual`, so wildcard builds do not fetch these archives. `:source_inputs`
exports the manifest and adapter for standalone consumers; `:sources` exposes
extracted archives with Bazel build metadata. Neither target compiles libraries.

Checksums establish input identity, not security or license approval. Native
compilation, final Cargo/Bazel linking, installed packages, minimum OS support
and duplex audio validation remain separate stages. These inputs do not establish
a shared Opus build with Rust consumers or a reduced dependency count.

Rust `opus` 0.4.0 is available through Socket. Adding Rust transport dependencies
and establishing a shared Opus build remain separate integration work.

## Native build recipe

`build_native.py` runs the unmodified upstream build systems in a new output
directory, using the same archives. Specify the target and existing compiler,
CMake, make, pkg-config and shell paths explicitly. It requires a matching
native host: GNU Linux, macOS, or Windows MSVC, on x64 or ARM64.

On macOS, specify the existing release deployment target with
`--deployment-target`; the host OS version is not an acceptable default.
Windows requires the normal Visual Studio SDK environment, Cygwin GNU make,
bash/cygpath and Automake 1.18's standard `ar-lib` for upstream libffi,
native Windows pkgconf, and `--bootstrap-make` pointing to NMake.
The recipe does not install these build prerequisites or patch upstream sources.
The private CI bootstrap verifies the official Cygwin installer and native pkgconf
MSI hashes before use. It also verifies a retained Cygwin package snapshot against
pinned archive and member hashes before installing it offline using signed
metadata. The installed package/version set must exactly match the snapshot
manifest.
The MSI is administratively extracted into job storage without a system install.
Cygwin runs under x64 emulation on ARM64; the compiler probes and emitted DLLs
must still match the real native target. Native pkgconf relocates libffi's POSIX
prefix metadata; CI rejects residual Cygwin paths. These are build prerequisites,
not shipped runtime components or evidence of working voice.

Outputs are under `prefix/`, build tools under `tools/`, and logs beside them.
`build-state.json` records completed commands and failures; `built.json` exists
only when every build/install command succeeds. Failed builds retain their logs
and must use a new output directory on retry. CMake compiler-identification logs
and the recorded tool/configuration inputs remain part of the build provenance.

The recipe disables optional plugins and Meson fallback dependency resolution,
with pkg-config restricted to this prefix. Only system ABI libraries/frameworks
may remain external; runtime closure inspection must verify that independently.
`//third_party/voice:build_inputs` exposes the recipe and source inputs to Bazel.
Neither this filegroup nor a successful prefix build proves final Cargo/Bazel
linkage, safe private runtime loading, or an installed voice-capable Codex package.
