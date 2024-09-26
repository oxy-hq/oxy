<p align="center"><img src="docs/readme-banner.png"/></p>

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
*Note: these are internal instructions in the absence of a brew formulae/cask. Steps 1-4 will be eventually subsumed within a single command, `brew install onyx/onyx`.*

To get started with `onyx`, you'll have to work within the terminal. By default you have a terminal on your macbook, which you can access through spotlight. If you want a faster experience, you can download tried-and-true [Iterm2](https://iterm2.com/), GPU-accelerated [Alacritty](https://github.com/alacritty/alacritty), or AI-native [Warp](https://warp.dev).

Once you have your terminal set up, following the commands below:

1. Install rust
```
curl https://sh.rustup.rs -sSf | sh
```

2. Clone this repository to your wherever you store your repos (I usually store mine in a `~/repos/`, which you can create and move into by running `cd ~ && mkdir repos && d repos`)
```
git clone git@github.com:onyx-hq/onyx-core.git
```

3. Build the crate from the repository
```
cargo build --release
```

4. Add the release to your path (replace `<YOUR_USERNAME>` with your username -- if you're in terminal you can figure out this out by typing `pwd`, and your home path with be printed out with your username):
```
export PATH="/Users/<YOUR_USERNAME>/.cargo/bin/::$PATH"
```

5. Finally, you need to clone the sample repo:
```
cd ~/repos/ && git clone git@github.com:onyx-hq/onyx-sample-repo.git
```

6. Then, copy the `config.yml` file into `~/.config/onyx/`:
```
mkdir -p ~/.config/onyx && ~/.config/onyx/config.yml
```

You'll have to modify the location of `warehouse.key_path` to be the location of your BigQuery service key (you can find credentials in [1password](https://hyperquery.1password.com/app#/lwrm73rxzjvbhi5hl3ludt2xcu/AllItems/lwrm73rxzjvbhi5hl3ludt2xcumv67bpwhm4f55j6e5k4y5jjnwa) for our Hyperquery data - or make your own).

You'll also need to change set the `OPENAI_API_KEY` environmental variable to an API key from OpenAI. You can create an API key [here](https://platform.openai.com/api-keys). Once you create one, you can save it by adding the following line to your `~/.bashrc` or `~/.zshrc` file:

```
export OPENAI_API_KEY="<YOUR_KEY_HERE>"
```

7. Restart your shell, and you should be good to go -- type `onyx` to check that the installation worked.

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

## The data directory

The `data` directory contains two important objects: `.sql` files, which are SQL definitions of business entities, and folders, which can be used to segment the queries and give the agents a natural sense of scope (they will have access to specific folder(s)).

The `.sql` files have front-matter (TODO: define what the front-matter should look like, and build it into the code).


## Agent definition (`agent.yml` configuration)
TODO: for now, see the `agents/default.yml` file in this repo or in the `onyx-sample-repo` directory.

## Contributing

### Language dependencies
Need to install Python and rust. Right now, Python is not being used, but the `build.rs` file and the `make build` command set up scaffolding if you want to incorporate Python. In general, we will follow the principle that everything should be in rust where possible for `onyx-core`. But this is obviously not possible for a number of integrations and data-specific tasks.

```
# Install python (w/asdf here)
asdf install python 3.11.6
asdf local python 3.11.6

# Install rustup (include cargo)
curl https://sh.rustup.rs -sSf | sh
```

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
