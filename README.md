<p align="center"><img src="docs/readme-banner.png"/></p>

# onyx core

## The fastest way to build data agents
`onyx` is a lightweight, yaml-based data agent builder for the command-line.

### What is a data agent?
A data agent is an LLM-based agent that can manipulate and synthesize data. `onyx` is a tool that allows you to build these agents quickly and easily, with a focus on flexibility and ease of use. 

Data agents built in `onyx` ingest semantic information about the business -- either auto-generated or maintained by an analytics domain expert -- and use this to either execute SQL queries against a database, retrieve from an in-process database, or retrieve from local sources (text, pdf, etc.)

`onyx` consists of two principle elements to configure these data agents:
- `agents`: Configuration files that define the agents. Each agent is scoped to particular semantic models.
- `semantic-models`: Configuration files that define a hierarchically scoped series of semantic definitions relevant to different scales of the business (from specific teams to the business as a whole).

## Quickstart
```bash
onyx init  # Initialize a folder as an onyx project
onyx bootstrap-semantics -f queries.sql  # Create some base entities.yml files inferred from a file of SQL queries

onyx "How many users do we have?"
We have 100 users.

onyx --output=code "How many users do we have?"
select count(distinct id_user) from dim_users
```

At this point, Onyx will function as a rudimentary text-to-SQL answer engine. To truly leverage the power of `onyx`, though, you'll need to understand how to configure two kinds of yaml configs within our platform: entities and agents.


## Semantic modeling (`entities.yml` configuration)

Semantic definitions live within a dbt-esque repository within the `semantic-models` directory, wherein there are `entities.yml` files, such as shown below.

```yaml
entities:
  - name: user  # use a single word that is intuitive for the business
    universal_key: id_user
  - name: organization
    universal_key: id_organization
  - name: country
    universal_key: dim_organization
  - name: page
    universal_key: id_page

calculations:
  - name: users
    sql: |
      select count(distinct {{entities.user.key}}) from core.dim_users
```

There are only three concepts to be aware of: `entities`, `metrics`, and `analyses`.

- **Entities:** if you’re familiar with Looker nomenclature, these are equivalent to dimensions, but the connotation we are going for is that these represent concrete, slowly-changing objects.
- **Metrics:** these are the equivalent of measures in Looker. These are metrics with a pertinent, generally stable business relevance. While these are a subset of analyses, it's important to distinguish them when possible.
- **Analyses:** these are just sql queries with descriptions. This includes metrics or more complex analyses.

### Semantic scoping

The `semantic-models` directory can also contain sub-directories which define **scope**. 

```jsx
.
*├── entities.yml*
└── **product**
    ├── **product-onboarding**
    *│   └── entities.yml
    └── entities.yml*
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
