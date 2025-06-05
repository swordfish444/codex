# Containerized Development

## Docker

To build the Docker image locally for x64 and then run it with the repo mounted:

```shell
CODEX_DOCKER_IMAGE_NAME=codex-linux-dev
docker build --platform=linux/amd64 -t "$CODEX_DOCKER_IMAGE_NAME" ./.devcontainer
docker run --platform=linux/amd64 --rm -it -v "$PWD":/app -w /app "$CODEX_DOCKER_IMAGE_NAME"
```

For arm64, specify `linux/arm64` instead.

Currently, the `Dockerfile` does not specify x64 vs. arm64, though you need to run `rustup target add x86_64-unknown-linux-musl` yourself to install the musl toolchain for x64.

## VS Code

If you open the workspace in a devcontainer in VS Code, in the terminal, you can build either flavor of the `arm64` build (GNU or musl):

```shell
cargo build --target aarch64-unknown-linux-musl
cargo build --target aarch64-unknown-linux-gnu
```
