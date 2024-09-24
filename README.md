<p align="center"><img src="docs/readme-banner.png"/></p>

## The fastest way to build data agents
`onyx` is a lightweight, yaml-based data agent builder for the command-line.

### What is a data agent?
A data agent is an LLM-based agent that can manipulate and synthesize data. `onyx` is a tool that allows you to build these agents quickly and easily, with a focus on flexibility and ease of use.

Data agents built in `onyx` ingest semantic information and use this to either execute SQL queries against a database, retrieve from an in-process database, or retrieve from local sources (text, pdf, etc.)

`onyx` consists of two principal elements to configure these data agents:
- `agents`: Configuration files that define the agents. Each agent is scoped to particular semantic models.
- `data`: A directory of SQL queries, organized in a directory structure that reflects the organization structure. Each SQL file should be placed in the sub-directory corresponding to the widest set of teams that it serves (company-wide metrics, for instance, should be placed in the base `data/` directory).

## Quickstart
Start by initializing a new repository with onyx:
```bash
onyx init  # Initialize a folder as an onyx project
```

This will construct a scaffolding within the current working directory with the necessary directories and a `default` agent for you to start working with.

This will also initialize a global config file in `~/.config/onyx/config.yml` (if it doesn't already exist). You should populate this with information with your warehouse credentials and LLM preferences.

Put some SQL queries into the `data` directory to seed the engine. An important aspect of `onyx` is the concept of `scope` -- the subset of the `data` directory that is available to an `agent`. By default, `onyx` comes with a way to access both the *narrowest* scope (that of a single query) and the *widest* scope (the entire `data` directory).

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

TODO

## Defining scope

NEEDS REWRITE

```jsx
.
├── entities.yml
└── product
    ├── product-onboarding
    │   └── entities.yml
    └── entities.yml
```

Here are how the scopes are defined in the example above:

- The `./entities.yml` file is the `base` scope.
- The `product` directory defines the `base.product` scope, and both `./entities.yml` and `./product/entities.yml` are used in the construction of queries.
- The `product-onboarding` directory defines the `base.product.product-onboarding` scope, and all of the entities files shown above are used towards the construction of queries.

This scoping mechanism follows a pattern that mirrors what we believe to be the optimal ownership hierarchy for data stewardship — every team inherits all scopes that are broader than themselves. This allows for efficient reuse and organization of entities while maintaining clear ownership boundaries. Each team or project can introduce their own entities at their specific scope level while still inheriting and utilizing the broader scope entities defined at higher levels.

These scopes are defined within the `config.yml` file at the base of the directory (see the Configuration section).
TODO: Enable scope inheritance to be turned off.

## Agent definition (`agent.yml` configuration)
Agents have four key properties:
- `model`: the model to be used, as specified within the `.config/onyx/config.yaml` file.
- `warehouse`: the warehouse against which queries are run, also specified in the `.config/onyx/config.yaml` file.
- `instructions`: a prompt given to the agent which can reference entities, metrics, and analyses using Jinja syntax
- `scope`: the scope of semantic models that the agent has access to.

## Contributing

### Language dependencies
Need to install Python and rust.
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
- ./onyx contains the backend-workspaces code (directly from titanium)
- ./src contains rust code
The CLI tool is built in Rust, and executes code from the Python backend code with `pyo3`. The choice of rust for the CLI tool primarily because the CLI is faster, and longer-term, we will want to extend the capabilities of the CLI to do things that are latency-sensitive, e.g. fuzzy-searching through command history, exploring results, viewing warehouse context.
