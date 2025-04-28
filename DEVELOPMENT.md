# Development

- [Development](#development)
  - [Clone This Repository](#clone-this-repository)
  - [Install Rust \& Node](#install-rust--node)
  - [Install Project Dependencies](#install-project-dependencies)
  - [Test Your Installation](#test-your-installation)
  - [Other Commands](#other-commands)
    - [Manual Testing Oxy Integration with Different Databases](#manual-testing-oxy-integration-with-different-databases)
  - [Known Issues](#known-issues)
  - [OxyPy Requirements](#oxypy-requirements)

## Clone This Repository

```sh
git clone git@github.com:oxy-hq/oxy.git
cd oxy
```

## Install Rust & Node

| :zap: **You are responsible for your own setup if you decide to not follow the instructions below.** |
| ---------------------------------------------------------------------------------------------------- |

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

```sh
cargo run -- --help
```

or you can start following our [quickstart guide](https://docs.oxy.tech/docs/quickstart).

## Other Commands

Please check out the docs folder for more commands and examples.

### Manual Testing Oxy Integration with Different Databases

We use Docker Compose to spin up database containers to create and supply test data. Ensure Docker and Docker Compose are installed on your system.

1. Start the required database containers:

   ```sh
   cd examples # make sure the docker compose you are trying to stand up is inside examples folder
   docker-compose up -d
   ```

2. Verify the containers are running:

   ```sh
   docker ps
   ```

3. Inside `examples` folder, you will find an agent for each database. Run the agent with the following command:

   ```sh
   cargo run -- run agents/<database>.agent.yml
   ```

   Replace `<database>` with the database you want to test.

4. To stop the containers after testing:

   ```sh
   docker-compose down
   ```

## Known Issues

- Tests when running in parallel (default) messed up terminal indentation.

## OxyPy Requirements

Ensure that the right Python version is in your path (oxyuses python3.11.6). There are many ways to install Python, here we recommend a few ways:

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
