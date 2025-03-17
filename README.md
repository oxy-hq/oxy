<p align="center"><img src="docs/readme-banner.png"/></p>

## The framework for agentic analytics

Oxy is an open-source framework for agentic analytics. It is declarative by design and written in Rust. Oxy is built with the following product principles in mind: open-source, performant, code-native, declarative, composable, and secure.

Agentic analytics applies software development lifecycle principles to AI-driven data analytics.
Just as traditional software follows a build-test-deploy pipeline, agentic analytics establishes a structured workflow for data agents, involving agent creation, prompt testing, and production deployment.

To learn more, read our [docs](https://docs.oxy.tech).

<https://github.com/user-attachments/assets/4b1efa5c-6b1b-4606-a47f-c9dac68cffad>

### Quickstart

To install Oxy from binary, run the following command (Mac, Linux, WSL):

```bash
bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/install_oxy.sh)
```

<details>
<summary>Alternative Installation Methods</summary>

#### Using Homebrew (macOS only)

```bash
brew install oxy-hqoxy/oxy
```

#### Installing a Specific Version

```bash
OXY_VERSION="0.1.0" bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/ooxy-hqxy/refs/heads/main/install_oxy.sh)
```

</details>

To verify the installation, run:

```bash
oxy --version
```

See our [docs](https://docs.oxy.tech) on how to modify the agent file, seed it with context, run tests, and create workflows.
