<p align="center"><img src="docs/readme-banner.png"/></p>


- [The fastest way to build data agents](#the-fastest-way-to-build-data-agents)
  - [What is a data agent?](#what-is-a-data-agent)
  - [Onyx core vision](#onyx-core-vision)
- [Quickstart from binary](#quickstart-from-binary)
- [Compile from source](#compile-from-source)
  - [Clone this repository](#clone-this-repository)
  - [Install rust \& node](#install-rust--node)
  - [Install project dependencies](#install-project-dependencies)
  - [Test your installation](#test-your-installation)
  - [Other commands](#other-commands)
  - [Run the desktop app](#run-the-desktop-app)
  - [Integration Testing](#integration-testing)
  - [Known issues](#known-issues)
- [Advanced setup after installation](#advanced-setup-after-installation)
  - [The data directory](#the-data-directory)
  - [Agent definition (`agent.yml` configuration)](#agent-definition-agentyml-configuration)
  - [Local LLM](#local-llm)
- [Onyx py requirements](#onyx-py-requirements)

## The fastest way to build data agents

`onyx` is a lightweight, yaml-based data agent builder for the command-line.

### What is a data agent?

A data agent is an LLM-based agent that can manipulate and synthesize data. `onyx` is a tool that allows you to build these agents quickly and easily, with a focus on flexibility and ease of use.

Data agents built in `onyx` ingest semantic information and use this to either execute SQL queries against a database, retrieve from an in-process database, or retrieve from local sources (text, pdf, etc.)

`onyx` consists of two principal elements to configure these data agents:

- `agents`: Configuration files that define the agents. Each agent is scoped to particular semantic models.
- `data`: A directory of SQL queries, organized in a directory structure that reflects the organization structure. Each SQL file should be placed in the sub-directory corresponding to the widest set of teams that it serves (company-wide metrics, for instance, should be placed in the base `data/` directory).

### Onyx core vision

`onyx` as-is provides two components: an `explorer` (`onyx search`) and an LLM interface that few-shots these entities into an LLM chatbot prompt chain (`onyx ask`).

The longer-term vision of `onyx` is such that the explorer provides search through a knowledge graph defining relevant entities (e.g. queries, metrics, dimensions, but perhaps an even wider scope, ultimately).

## Quickstart from binary

To install `onyx` from a binary, you can use the following commands:

- For linux and macOS:

```bash
bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/onyx-hq/onyx-public-releases/refs/heads/main/install_onyx.sh)
```

- For windows:

```powershell
powershell -Command "& { iwr -useb https://raw.githubusercontent.com/onyx-hq/onyx-public-releases/refs/heads/main/install_onyx.ps1 | iex }"
```

## Compile from source

### Clone this repository

```sh
git clone git@github.com:onyx-hq/onyx.git
cd onyx
```

### Install rust & node

| :zap:        **you are responsible for your own setup if you decide to not follow the instructions below.**   |
|-----------------------------------------|

Install rust by following the official guide [here](https://www.rust-lang.org/tools/install).

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Add cargo to your path by following the instructions at the end of the installation. Or:

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

Install node by following the official guide [here](https://nodejs.org/en/download/) or using mise.
We recommend using LTS version.
We recommend using mise to install node and manage versions, to isolate the node environment from the system.

```sh
# Using mise is a recommended way to install node and manage versions
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

### Install project dependencies

After rust and nodejs are installed, install project dependencies:

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

### Test your installation

Download our bigquery key in [1password](https://hyperquery.1password.com/app#/lwrm73rxzjvbhi5hl3ludt2xcu/AllItems/lwrm73rxzjvbhi5hl3ludt2xcumv67bpwhm4f55j6e5k4y5jjnwa) and save it into `examples/bigquery-sample.key`

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

### Other commands

Please checkout the docs folder for more commands and examples.

Also, update the docs folder with usability docs and keep this README lean.

### Run the desktop app

To run the desktop app, you can use the following command:

```bash
pnpm tauri dev
```

Follow tauri upstream documents for more commands [with tauri-cli](https://v2.tauri.app/reference/cli/)

### Integration Testing

Test are running on examples configuration.
To run all tests, need `bigquery_sample.key` in `examples` directory and `OPENAI_API_KEY` env set.

```sh
cargo test
```

To show test stdout for debugging use `--nocapture`

```sh
cargo test -- --nocapture
```

### Known issues

- Tests when running in parallel (default) messed up terminal indentation.

---

> [!WARNING]  
> You can stop here and get to work with `onyx` as it is. The following steps are for advanced users who want to set up a new repository and configure agents.

---

## Advanced setup after installation

Start by initializing a new repository with onyx:

```bash
onyx init  # Initialize a folder as an onyx project
```

This will construct a scaffolding within the current working directory with the necessary directories and a `default` agent for you to start working with.

Put some SQL queries into the `data` directory to seed the engine.

An important aspect of `onyx` is the concept of `scope` -- the subset of the `data` directory that is available to an `agent`. By default, `onyx` comes with a way to access both the _narrowest_ scope (that of a single query) and the _widest_ scope (the entire `data` directory).

```bash
onyx build
```

Then verify using

```bash
onyx vec-search "Hello Embedding"
```

### The data directory

The `data` directory contains two important objects: `.sql` files, which are SQL definitions of business entities, and folders, which can be used to segment the queries and give the agents a natural sense of scope (they will have access to specific folder(s)).

The `.sql` files have front-matter (TODO: define what the front-matter should look like, and build it into the code).

### Agent definition (`agent.yml` configuration)

TODO: for now, see the `agents/default.yml` file in this repo or in the `onyx-sample-repo` directory.

### Local LLM

- To start local LLM inference you can download and install `ollama`
- Add model config into `config.yml` file:

```
- name: ollama-local
  vendor: ollama
  model_ref: llama3.2:latest
```

and update agent's model config:

```
model: ollama-local
```

- Local LLM agent needs to support tools.

## Onyx py requirements

Ensure that the right python version is in your path (onyx uses python3.11.6). There are many ways to install python, here we recommend a few ways:

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

Poetry is a python package manager that we use to manage python dependencies. Install poetry by following the official guide [here](https://python-poetry.org/docs/). Or you can install it with pip:

```sh
pip install poetry
```
