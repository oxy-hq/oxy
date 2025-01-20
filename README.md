<p align="center"><img src="docs/readme-banner.png"/></p>

- [The fastest way to build data agents](#the-fastest-way-to-build-data-agents)
  - [What is a data agent?](#what-is-a-data-agent)
  - [Onyx core vision](#onyx-core-vision)
- [Quickstart from binary](#quickstart-from-binary)
- [Compile from source](#compile-from-source)
  - [Clone this repository](#clone-this-repository)
  - [Install rust \& node](#install-rust--node)
  - [Build the crate from the repository](#build-the-crate-from-the-repository)
  - [Run the desktop app](#run-the-desktop-app)
  - [Test the installation](#test-the-installation)
- [Starting from scratch](#starting-from-scratch)
- [Basic commands](#basic-commands)
- [The data directory](#the-data-directory)
- [Agent definition (`agent.yml` configuration)](#agent-definition-agentyml-configuration)
- [Local LLM](#local-llm)
- [Contributing](#contributing)
  - [Language dependencies](#language-dependencies)
  - [Extra - install python \& poetry](#extra---install-python--poetry)
- [Testing](#testing)
  - [Known issues](#known-issues)

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

cd ../  # Move up a directory to ensure that the sample repo is not nested within the onyx repo
git clone git@github.com:onyx-hq/onyx-sample-repo.git
cd onyx-sample-repo
```

### Install rust & node

The best way to manage rust is with rustup. Install rust by following the official guide [here](https://www.rust-lang.org/tools/install).

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

Node is used for `onyx serve` command. You can install it by following the official guide [here](https://nodejs.org/en/download/).
We recommend using version 20, but you can use any version that is compatible with the project.

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

After nodejs is installed, install project dependencies:

```sh
corepack enable && corepack prepare --activate
pnpm install
```

### Build the crate from the repository

Inside onyx

```sh
# this will build everything, incl the frontend for onyx serve
pnpm run build

# after the first build, you can use this to build faster
# this will just build the CLI
cargo build --release

# this will just build the frontend
pnpm -C web-app build
```

After building, you can install the binary to your path:

```sh
# Install the binary to your path
cp target/release/onyx /usr/local/bin/onyx
```

Then, copy the `config.yml` file into `~/.config/onyx/`:

```sh
mkdir -p ~/.config/onyx && cp example.yml ~/.config/onyx/config.yml
```

You'll have to modify the location of `warehouse.key_path` to be the location of your BigQuery service key (you can find credentials in [1password](https://hyperquery.1password.com/app#/lwrm73rxzjvbhi5hl3ludt2xcu/AllItems/lwrm73rxzjvbhi5hl3ludt2xcumv67bpwhm4f55j6e5k4y5jjnwa) for our Hyperquery data - or make your own). and modify the `project_path` to onyx-sample-repo

```sh
# Modify the location of the key_path in the config.yml file
open ~/.config/onyx/config.yml
```

Set the `OPENAI_API_KEY` environmental variable to an API key from OpenAI. You can create an API key [here](https://platform.openai.com/api-keys). Once you create one, you can save it by adding the following line to your `~/.bashrc` or `~/.zshrc` file:

```sh
export OPENAI_API_KEY="<YOUR_KEY_HERE>"

# save it into zshrc
# so that the next time you open a terminal, it will be available
echo 'export OPENAI_API_KEY="<YOUR_KEY_HERE>"' >> ~/.zshrc
```

### Run the desktop app

To run the desktop app, you can use the following command:

```bash
pnpm -C web-app tauri dev
```

Follow tauri upstream documents for more commands [with tauri-cli](https://v2.tauri.app/reference/cli/)

### Test the installation

Inside onyx-sample-repo, run: `onyx` to check that the installation worked.

## Starting from scratch

Start by initializing a new repository with onyx:

```bash
onyx init  # Initialize a folder as an onyx project
```

This will construct a scaffolding within the current working directory with the necessary directories and a `default` agent for you to start working with.

This will also initialize a global config file in `~/.config/onyx/config.yml` (if it doesn't already exist). You should populate this with information with your warehouse credentials and LLM preferences.

Put some SQL queries into the `data` directory to seed the engine.

## Basic commands

An important aspect of `onyx` is the concept of `scope` -- the subset of the `data` directory that is available to an `agent`. By default, `onyx` comes with a way to access both the _narrowest_ scope (that of a single query) and the _widest_ scope (the entire `data` directory).

To access SQL-level scope, you can run:

```bash
onyx search  # Search through your SQL queries and execute them on selection
```

which will execute the selected query.

To access the org-wide scope, you can ask questions of the `default` LLM, which will source from the `data` base directory (scope: `all`) by default.

```bash
onyx ask "How many users do we have?"  # Ask a question of the default agent
```

If you want to access a specific agent (for either `search` or `ask`), you can do so by specifying the agent name (removing the `.yml` extension) as an argument.

```bash
onyx ask --agent=default "How many users do we have?"  # Ask a question of the specific agent `default.yaml`
```

To embed files from <project_root>/data/\*\* into vector store you can use `onyx build`. We're downloading model from huggingface hub so you may need to login using:

```bash
huggingface-cli login
```

or simply copy your plaintext token into `$HOME/.cache/huggingface/token` file. And then run build to index the data.

```bash
onyx build
```

Then verify using

```bash
onyx vec-search "Hello Embedding"
```

## The data directory

The `data` directory contains two important objects: `.sql` files, which are SQL definitions of business entities, and folders, which can be used to segment the queries and give the agents a natural sense of scope (they will have access to specific folder(s)).

The `.sql` files have front-matter (TODO: define what the front-matter should look like, and build it into the code).

## Agent definition (`agent.yml` configuration)

TODO: for now, see the `agents/default.yml` file in this repo or in the `onyx-sample-repo` directory.

## Local LLM

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

## Contributing

### Language dependencies

Need to install node and rust (_find instruction above_). Right now, Python is not being used, but the `build.rs` file contains some hint for scaffolding if you want to incorporate Python. In general, we will follow the principle that everything should be in rust where possible for `onyx`. But this is obviously not possible for a number of integrations and data-specific tasks.

### Extra - install python & poetry

Ensure that the right python version is in your path (onyx uses python3.11.6). There are many ways to install python, here we recommend a few ways:

```sh
# Using asdf is best for amd64
brew install asdf

case $SHELL in
  *bash)
    echo -e "\n. $(brew --prefix asdf)/asdf.sh" >> ~/.bashrc
    source ~/.bashrc
    ;;
  *zsh)
    echo -e "\n. $(brew --prefix asdf)/asdf.sh" >> ~/.zshrc
    source ~/.zshrc
    ;;
esac

asdf plugin add python
asdf install python 3.11.6

asdf global python 3.11.6
```

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

## Testing

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
