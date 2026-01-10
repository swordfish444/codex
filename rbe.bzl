def _rbe_platform_settings_for_arch(arch):
    if arch in ["x86_64", "amd64"]:
        return struct(cpu = "x86_64", exec_arch = "amd64")
    if arch in ["aarch64", "arm64"]:
        return struct(cpu = "aarch64", exec_arch = "arm64")
    fail("Unsupported host arch for rbe platform: {}".format(arch))

def _rbe_platform_repo_impl(rctx):
    print(rctx.attr.name)
    settings = _rbe_platform_settings_for_arch(rctx.os.arch)
    rctx.file("BUILD.bazel", """\
platform(
    name = "rbe_platform",
    constraint_values = [
        "@platforms//cpu:{cpu}",
        "@platforms//os:linux",
        "@bazel_tools//tools/cpp:clang",
        "@toolchains_llvm_bootstrapped//constraints/libc:gnu.2.28",
    ],
    exec_properties = {{
        # Ubuntu-based image that includes git, python3, dotslash, and other
        # tools that various integration tests need.
        # Verify at https://hub.docker.com/layers/mbolin491/codex-bazel/latest/images/sha256:ad9506086215fccfc66ed8d2be87847324be56790ae6a1964c241c28b77ef141
        "container-image": "docker://docker.io/mbolin491/codex-bazel@sha256:ad9506086215fccfc66ed8d2be87847324be56790ae6a1964c241c28b77ef141",
        "Arch": "{arch}",
        "OSFamily": "Linux",
    }},
)
""".format(
    cpu = settings.cpu,
    arch = settings.exec_arch,
))

rbe_platform_repository = repository_rule(
    implementation = _rbe_platform_repo_impl,
    doc = "Sets up a platform for remote builds with an Arch exec_property matching the host.",
)
