<p align="center"><img src="docs/readme-banner.png"/></p>


- [The fastest way to build data agents](#the-fastest-way-to-build-data-agents)
  - [What is a data agent?](#what-is-a-data-agent)
  - [Onyx core vision](#onyx-core-vision)
- [Quickstart](#quickstart)
  - [Install rust](#install-rust)
  - [Install python](#install-python)
  - [Install poetry](#install-poetry)
  - [Clone this repository](#clone-this-repository)
  - [Build the crate from the repository](#build-the-crate-from-the-repository)
  - [Test the installation](#test-the-installation)
- [Starting from scratch](#starting-from-scratch)
- [Basic commands](#basic-commands)
- [The data directory](#the-data-directory)
- [Agent definition (`agent.yml` configuration)](#agent-definition-agentyml-configuration)
- [Contributing](#contributing)
  - [Language dependencies](#language-dependencies)
  - [Build](#build)
  - [Repository structure](#repository-structure)

## The fastest way to build data agents

`onyx` is a lightweight, yaml-based data agent builder for the command-line.

### What is a data agent?

A data agent is an LLM-based agent that can manipulate and synthesize data. `onyx` is a tool that allows you to build these agents quickly and easily, with a focus on flexibility and ease of use.

Data agents built in `onyx` ingest semantic information and use this to either execute SQL queries against a database, retrieve from an in-process database, or retrieve from local sources (text, pdf, etc.)

`onyx` consists of two principal elements to configure these data agents:

- `agents`: Configuration files that define the agents. Each agent is scoped to particular semantic models.
- `data`: A directory of SQL queries, organized in a directory structure that reflects the organization structure. Each SQL file should be placed in the sub-directory corresponding to the widest set of teams that it serves (company-wide metrics, for instance, should be placed in the base `data/` directory).

### Onyx core vision

`onyx-core` as-is provides two components: an `explorer` (`onyx search`) and an LLM interface that few-shots these entities into an LLM chatbot prompt chain (`onyx ask`).

The longer-term vision of `onyx-core` is such that the explorer provides search through a knowledge graph defining relevant entities (e.g. queries, metrics, dimensions, but perhaps an even wider scope, ultimately).

## Quickstart

*Note: these are internal instructions in the absence of a brew formulae/cask. Some steps will be eventually subsumed within a single command, `brew install onyx/onyx`.*

To get started with `onyx`, you'll have to work within the terminal. By default you have a terminal on your macbook, which you can access through spotlight. If you want a faster experience, you can download tried-and-true [Iterm2](https://iterm2.com/), GPU-accelerated [Alacritty](https://github.com/alacritty/alacritty), or AI-native [Warp](https://warp.dev).

Once you have your terminal set up, following the commands below:

### Install rust

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

### Install python

Ensure that the right python version is in your path (onyx-core uses python3.11.6). There are many ways to install python, here we recommend a few ways:

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

### Install poetry

Poetry is a python package manager that we use to manage python dependencies. Install poetry by following the official guide [here](https://python-poetry.org/docs/). Or you can install it with pip:

```sh
pip install poetry
```

### Clone this repository

```sh
git clone git@github.com:onyx-hq/onyx-core.git
cd onyx-core

cd ../  # Move up a directory to ensure that the sample repo is not nested within the onyx-core repo
git clone git@github.com:onyx-hq/onyx-sample-repo.git
cd onyx-sample-repo
```

### Build the crate from the repository

Inside onyx-core

```sh
cargo build --release
```

Copy the binary to your path:

```sh
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

An important aspect of `onyx` is the concept of `scope` -- the subset of the `data` directory that is available to an `agent`. By default, `onyx` comes with a way to access both the *narrowest* scope (that of a single query) and the *widest* scope (the entire `data` directory).

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

To embed files from <project_root>/data/** into vector store you can use `onyx build`. We're downloading model from huggingface hub so you may need to login using:

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

## Contributing

### Language dependencies

Need to install Python and rust (*find instruction above*). Right now, Python is not being used, but the `build.rs` file and the `make build` command set up scaffolding if you want to incorporate Python. In general, we will follow the principle that everything should be in rust where possible for `onyx-core`. But this is obviously not possible for a number of integrations and data-specific tasks.

### Build

Run `make build` to build.
So long as the python code doesn't change, `make build` only needs to be run once, and then we can just run `cargo build` to update the CLI.

The build sequencing is as follows:

- The python modules are installed using `poetry` to a virtual environment.
- The rust crate is built, and uses `pyo3` to execute the code *using the virtual environment that was made in the previous step*.

### Repository structure

This repository is a mixed Python/rust repository.

- ./onyx contains python code
- ./src contains rust code
The CLI tool is built in Rust, and executes code from the Python backend code with `pyo3`. The choice of rust for the CLI tool came primarily to optimize for user experience and technical defensibility, as opposed to optimizing for leveraging community contributions for development. See the decision doc [here](https://www.notion.so/hyperquery/Why-Rust-for-CLI-front-end-10c13791d2b580f2afe2c9b2d2c663ea) for full context.
