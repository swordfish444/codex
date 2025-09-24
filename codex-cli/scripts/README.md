# npm releases

Run the following:

To build the 0.2.x or later version of the npm module, which runs the Rust version of the CLI, build it as follows:

```bash
./codex-cli/scripts/build_npm_package.py --release-version 0.6.0
```

To produce per-platform "slice" tarballs in addition to the fat package, supply the
`--slice-pack-dir` flag to write the outputs. For example:

```bash
./codex-cli/scripts/build_npm_package.py --release-version 0.6.0 --slice-pack-dir dist/npm
```

The command above writes the full tarball plus the per-platform archives named with the
VS Code-style identifiers (for example, `codex-npm-0.6.0-darwin-arm64.tgz`). Note this will
create `./codex-cli/vendor/` as a side-effect.
