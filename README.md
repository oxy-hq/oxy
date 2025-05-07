<p align="center"><img src="docs/readme-banner.png"/></p>

<p align="center">
<a href="#license"><img src="https://img.shields.io/badge/License-Apache--2.0-blue" alt="License"></a>
<a href="https://discord.gg/m677N4EcRK"><img src="https://img.shields.io/discord/1344823951020527638?label=Discord&logo=discord&color=7289da" alt="Join us on Discord"></a>
</p>

<div align="center">
<a href="https://docs.oxy.tech" title="Go to project documentation"><img src="https://img.shields.io/badge/view-Documentation-blue?style=for-the-badge" alt="view - Documentation"></a>
</div>

> 📖 **Looking for up-to-date code documentation?**  
> Check out our [repository wiki](https://deepwiki.com/oxy-hq/oxy), which auto-refreshes weekly with the latest code changes.

## The framework for agentic analytics

Oxy is an open-source framework for agentic analytics. It is declarative by design and written in Rust. Oxy is built with the following product principles in mind: open-source, performant, code-native, declarative, composable, and secure.

Agentic analytics applies software development lifecycle principles to AI-driven data analytics.
Just as traditional software follows a build-test-deploy pipeline, agentic analytics establishes a structured workflow for data agents, involving agent creation, prompt testing, and production deployment.

To learn more, read our [docs](https://docs.oxy.tech).

<https://github.com/user-attachments/assets/40ce54c6-810a-4843-ace1-0e57c0d2ae71>

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

</details>

To verify the installation, run:

```bash
oxy --version
```

See our [docs](https://docs.oxy.tech) on how to modify the agent file, seed it with context, run tests, and create workflows.
