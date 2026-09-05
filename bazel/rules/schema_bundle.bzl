"""Cache schema generation from the filtered public Rust test executable."""

def _schema_bundle_impl(ctx):
    outputs = []
    for mode in ["stable", "experimental"]:
        schema = ctx.actions.declare_directory(ctx.label.name + "." + mode)
        ctx.actions.run(
            executable = ctx.executable.generator,
            tools = [ctx.attr.generator[DefaultInfo].files_to_run],
            arguments = [
                "--exact",
                "--ignored",
                "schema_fixtures_tests::write_schema_fixtures_from_env",
            ],
            env = {
                "CODEX_APP_SERVER_SCHEMA_ROOT": schema.path,
                "CODEX_APP_SERVER_SCHEMA_EXPERIMENTAL": "1" if mode == "experimental" else "0",
                "RUST_MIN_STACK": "8388608",
            },
            outputs = [schema],
            mnemonic = "PublicSchema",
            progress_message = "Generating %s public app-server schemas" % mode,
        )
        outputs.append(schema)

    # Expose the pinned compressor alongside the cached directories so callers
    # can normalize the JSON without a separate `bazel run` invocation.
    zstd = ctx.actions.declare_file(ctx.label.name + ".zstd")
    ctx.actions.symlink(output = zstd, target_file = ctx.executable.zstd, is_executable = True)
    return [DefaultInfo(files = depset(outputs + [zstd]))]

schema_bundle = rule(
    implementation = _schema_bundle_impl,
    attrs = {
        "generator": attr.label(mandatory = True, executable = True, cfg = "exec"),
        "zstd": attr.label(default = "@zstd//:zstd_cli", executable = True, cfg = "exec"),
    },
    doc = "Generates both schema modes as cacheable actions with explicit tool dependencies.",
)
