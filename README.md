<p align="center"><img src="assets/readme-banner.png"/></p>

<p align="center">
<a href="#license"><img src="https://img.shields.io/badge/License-Apache--2.0-blue" alt="License"></a>
</p>

<div align="center">
<a href="https://oxy.tech/docs" title="Go to project documentation"><img src="https://img.shields.io/badge/view-Documentation-blue?style=for-the-badge" alt="view - Documentation"></a>
</div>

> 📖 **Looking for up-to-date code documentation?**  
> Check out our DeepWiki, which updates weekly with the latest code changes: [![DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/oxy-hq/oxy)

## The framework for agentic analytics

Oxy is an open-source framework for building comprehensive agentic analytics systems grounded in deterministic execution principles. Written in Rust and declarative by design, Oxy provides the foundational components needed to transform AI-driven data analysis into reliable, production-ready systems through structured primitives, semantic understanding, and predictable execution.

To learn more, read our [docs](https://oxy.tech/docs).

### Quickstart

To install Oxy from binary, run the following command (Mac, Linux, WSL):

```bash
bash <(curl --proto '=https' --tlsv1.2 -LsSf https://get.oxy.tech)
```

<details>
<summary>Alternative Installation Methods</summary>

#### Using Homebrew (macOS only)

```bash
brew install oxy-hq/oxy/oxy
```

#### Installing a Specific Version

```bash
OXY_VERSION="0.1.0" bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/install_oxy.sh)
```

#### Installing Nightly/Edge Builds

To install the latest edge build (built from main branch):

```bash
bash <(curl --proto '=https' --tlsv1.2 -LsSf https://nightly.oxy.tech)
```

To install the latest nightly build (scheduled daily):

```bash
OXY_CHANNEL=nightly bash <(curl --proto '=https' --tlsv1.2 -LsSf https://nightly.oxy.tech)
```

To install a specific edge or nightly version:

```bash
# Install specific edge build
OXY_VERSION=edge-7cbf0a5 bash <(curl --proto '=https' --tlsv1.2 -LsSf https://nightly.oxy.tech)

# Install specific nightly build
OXY_VERSION=nightly-20251204-abc1234 bash <(curl --proto '=https' --tlsv1.2 -LsSf https://nightly.oxy.tech)
```

Browse all available nightly and edge releases at [oxy-hq/oxy-nightly](https://github.com/oxy-hq/oxy-nightly/releases).

</details>

To verify the installation, run:

```bash
oxy --version
```

## Database

Oxy uses PostgreSQL for all deployments. For local development, an embedded PostgreSQL instance starts automatically - no setup required!

For production deployments, configure an external PostgreSQL database:

```bash
export OXY_DATABASE_URL=postgresql://user:password@host:port/database
```

See [DEVELOPMENT.md](DEVELOPMENT.md#database) for more details about database configuration and migration.

---

See our [docs](https://oxy.tech/docs) on how to modify the agent file, seed it with context, run tests, and create workflows.
