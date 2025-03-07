# Development

- [Development](#development)
  - [Clone This Repository](#clone-this-repository)
  - [Install Rust \& Node](#install-rust--node)
  - [Install Project Dependencies](#install-project-dependencies)
  - [Test Your Installation](#test-your-installation)
  - [Other Commands](#other-commands)
  - [Integration Testing](#integration-testing)
  - [Known Issues](#known-issues)
  - [Onyx Py Requirements](#onyx-py-requirements)

## Clone This Repository

```sh
git clone git@github.com:onyx-hq/onyx.git
cd onyx
```

## Install Rust & Node

| :zap:        **You are responsible for your own setup if you decide to not follow the instructions below.**   |
|---------------------------------------------------------------------------------------------------------------|

Install Rust by following the official guide [here](https://www.rust-lang.org/tools/install).

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Add Cargo to your path by following the instructions at the end of the installation. Or:

```bash
# Detect the current shell
current_shell=$(basename "$SHELL")

# Append the PATH export line to the appropriate shell profile
case "$current_shell" in
  bash)
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
    source ~/.bashrc
    ;;
  zsh)
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
    source ~/.zshrc
    ;;
  *)
    echo "Unsupported shell: $current_shell"
    ;;
esac
```

Install Node by following the official guide [here](https://nodejs.org/en/download/) or using mise.
We recommend using the LTS version.
We recommend using mise to install Node and manage versions, to isolate the Node environment from the system.

```sh
# Using mise is a recommended way to install Node and manage versions
curl https://mise.run | sh


case $SHELL in
  *bash)
    echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc
    source ~/.bashrc
    ;;
  *zsh)
    echo 'eval "$(~/.local/bin/mise activate zsh)"' >> ~/.zshrc
    source ~/.zshrc
    ;;
esac

# inside this repo
mise install
```

## Install Project Dependencies

After Rust and Node.js are installed, install project dependencies:

```sh
corepack enable && corepack prepare --activate
pnpm install

# this will just build the CLI
cargo build

# this will just build the frontend
pnpm -C web-app build

# a shortcut for building both the CLI and the frontend
# pnpm build
```

> **Important:** Ensure you have the necessary keys and environment variables set up before running your first query.

## Test Your Installation

Download our BigQuery key in [1password](https://hyperquery.1password.com/app#/lwrm73rxzjvbhi5hl3ludt2xcu/AllItems/lwrm73rxzjvbhi5hl3ludt2xcumv67bpwhm4f55j6e5k4y5jjnwa) and save it into `examples/bigquery-sample.key`

Inside 1password, find the `OPENAI_API_KEY` and set it in your environment:

```sh
# consider adding this to mise or your local shell to persist it
export OPENAI_API_KEY=sk-...
```

Run your first query:

```sh
onyx run agents/default.agent.yml "how many users"
```

Try other commands:

```sh
onyx --help
```

## Other Commands

Please check out the docs folder for more commands and examples.

Also, update the docs folder with usability docs and keep this README lean.

## Integration Testing

Tests are running on examples configuration.
To run all tests, need `bigquery_sample.key` in `examples` directory and `OPENAI_API_KEY` env set.

```sh
cargo test
```

To show test stdout for debugging use `--nocapture`

```sh
cargo test -- --nocapture
```

## Known Issues

- Tests when running in parallel (default) messed up terminal indentation.

## Onyx Py Requirements

Ensure that the right Python version is in your path (onyx uses python3.11.6). There are many ways to install Python, here we recommend a few ways:

```sh
# Using mise is the best for any architecture
curl https://mise.run | sh


case $SHELL in
  *bash)
    echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc
    source ~/.bashrc
    ;;
  *zsh)
    echo 'eval "$(~/.local/bin/mise activate zsh)"' >> ~/.zshrc
    source ~/.zshrc
    ;;
esac

mise install python 3.11.6

mise use python@3.11 --global

```

```sh
# Using brew
brew install python@3.11
brew link python@3.11
```

Poetry is a Python package manager that we use to manage Python dependencies. Install Poetry by following the official guide [here](https://python-poetry.org/docs/). Or you can install it with pip:

```sh
pip install poetry
```
