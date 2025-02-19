<p align="center"><img src="docs/readme-banner.png"/></p>

<p align="center">
  <a href="https://github.com/onyx-hq/onyx/actions/workflows/ci.yaml">
    <img src="https://github.com/onyx-hq/onyx/actions/workflows/ci.yaml/badge.svg" alt="CI Status">
  </a>
</p>

- [The Fastest Way to Build Data Agents](#the-fastest-way-to-build-data-agents)
  - [What is a Data Agent?](#what-is-a-data-agent)
  - [Onyx Core Vision](#onyx-core-vision)
- [Quickstart from Binary](#quickstart-from-binary)
- [Advanced Setup After Installation](#advanced-setup-after-installation)
  - [The Data Directory](#the-data-directory)
  - [Agent Definition (`agent.yml` Configuration)](#agent-definition-agentyml-configuration)
  - [Local LLM](#local-llm)

## The Fastest Way to Build Data Agents

`onyx` is a lightweight, YAML-based data agent builder for the command-line.

### What is a Data Agent?

A data agent is an LLM-based agent that can manipulate and synthesize data. `onyx` is a tool that allows you to build these agents quickly and easily, with a focus on flexibility and ease of use.

Data agents built in `onyx` ingest semantic information and use this to either execute SQL queries against a database, retrieve from an in-process database, or retrieve from local sources (text, PDF, etc.)

`onyx` consists of two principal elements to configure these data agents:

- `agents`: Configuration files that define the agents. Each agent is scoped to particular semantic models.
- `data`: A directory of SQL queries, organized in a directory structure that reflects the organization structure. Each SQL file should be placed in the sub-directory corresponding to the widest set of teams that it serves (company-wide metrics, for instance, should be placed in the base `data/` directory).

### Onyx Core Vision

`onyx` as-is provides two components: an `explorer` (`onyx search`) and an LLM interface that few-shots these entities into an LLM chatbot prompt chain (`onyx ask`).

The longer-term vision of `onyx` is such that the explorer provides search through a knowledge graph defining relevant entities (e.g. queries, metrics, dimensions, but perhaps an even wider scope, ultimately).

## Quickstart from Binary

To install `onyx` from a binary, you can use the following commands:

- For macOS:

```bash
bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/onyx-hq/onyx-public-releases/refs/heads/main/install_onyx.sh)
```

- Linux and Windows support are coming soon.

For usage instructions, check out our documentation at [docs.onyxint.io](https://docs.onyxint.ai).
For development instructions, check out the `DEVELOPMENT.md` file in this repository.

## Advanced Setup After Installation

Start by initializing a new repository with `onyx`:

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

### The Data Directory

The `data` directory contains two important objects: `.sql` files, which are SQL definitions of business entities, and folders, which can be used to segment the queries and give the agents a natural sense of scope (they will have access to specific folder(s)).

The `.sql` files have front-matter (TODO: define what the front-matter should look like, and build it into the code).

### Agent Definition (`agent.yml` Configuration)

TODO: for now, see the `agents/default.yml` file in this repo or in the `onyx-sample-repo` directory.

### Local LLM

- To start local LLM inference you can download and install `ollama`
- Add model config into `config.yml` file:

```yaml
- name: ollama-local
  vendor: ollama
  model_ref: llama3.2:latest
```

and update agent's model config:

```yaml
model: ollama-local
```

- Local LLM agent needs to support tools.
