<p align="center"><img src="assets/readme-banner.png"/></p>

<p align="center">
<a href="#license"><img src="https://img.shields.io/badge/License-Apache--2.0-blue" alt="License"></a>
</p>

<div align="center">
<a href="https://oxy.tech/docs" title="Go to project documentation"><img src="https://img.shields.io/badge/view-Documentation-blue?style=for-the-badge" alt="view - Documentation"></a>
</div>

> 📖 **Looking for up-to-date code documentation?**  
> Check out our DeepWiki, which updates weekly with the latest code changes: [![DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/oxy-hq/oxy)

## The Open-source, Agent-Native Data Platform 

Oxygen is the Full-stack Data + AI Platform purpose-built from first principles for Agentic Data Analytics. We combine a data lakehouse, an ETL engine, a data modeling engine (ontology engine), an agent and automation engine, and an agentic application engine to become a one-stop shop for anything Data and AI.

To learn more, read our [docs](https://oxy.tech/docs).

### Quickstart

To install Oxy from binary, run the following command (Mac, Linux, WSL):

```bash
bash <(curl -sSfL https://get.oxy.tech)
```

<details>
<summary>Alternative Installation Methods</summary>

#### Using Homebrew (macOS only)

```bash
brew install oxy-hq/oxy/oxy
```

#### Installing a Specific Version

```bash
OXY_VERSION="0.1.0" bash <(curl -sSfL https://get.oxy.tech)
```

#### Installing Edge Builds

To install the latest edge build (built from main branch):

```bash
bash <(curl -sSfL https://nightly.oxy.tech)
```

To install a specific edge version:

```bash
OXY_VERSION=edge-7cbf0a5 bash <(curl -sSfL https://nightly.oxy.tech)
```

#### Browsing Available Releases

To list all available releases across stable and edge channels:

```bash
bash <(curl -sSfL https://release.oxy.tech)
```

Filter by channel or adjust the number of results:

```bash
bash <(curl -sSfL https://release.oxy.tech) --channel stable
bash <(curl -sSfL https://release.oxy.tech) -c edge -n 20
```

You can also browse releases directly on GitHub: [stable](https://github.com/oxy-hq/oxy/releases) | [edge](https://github.com/oxy-hq/oxy-nightly/releases).

</details>

To verify the installation, run:

```bash
oxy --version
```

## Quick Deploy

Deploy the complete Oxy demo application with one click:

This deployment includes:

- ✅ Complete Oxy application (Rust backend + React frontend)
- ✅ Demo retail analytics project with Oxymart dataset
- ✅ Pre-configured workflows and data apps
- ✅ Persistent storage for databases
- ✅ Free tier available

### Deployment Steps

1. **Prerequisites**: Install the [Fly CLI](https://fly.io/docs/hands-on/install-flyctl/)

   ```bash
   curl -L https://fly.io/install.sh | sh
   ```

2. **Login to Fly.io**:

   ```bash
   fly auth login
   ```

3. **Deploy**:

   ```bash
   fly launch
   ```

   Follow the prompts to:
   - Choose your app name and region
   - Create a persistent volume for data storage
   - The deployment will automatically use `Dockerfile.demo` with the demo_project included

4. **Set your API key** (required for AI features):

   ```bash
   fly secrets set OPENAI_API_KEY=sk-your-key-here
   ```

5. **Access your app**:

   ```bash
   fly open
   ```

Your Oxy instance will be live at `https://your-app-name.fly.dev` with the complete demo project ready to explore!

## Database

Oxy uses PostgreSQL for all deployments. For local development, an embedded PostgreSQL instance starts automatically - no setup required!

For production deployments, configure an external PostgreSQL database:

```bash
export OXY_DATABASE_URL=postgresql://user:password@host:port/database
```

See [DEVELOPMENT.md](DEVELOPMENT.md#database) for more details about database configuration and migration.

---

See our [docs](https://oxy.tech/docs) on how to modify the agent file, seed it with context, run tests, and create workflows.
