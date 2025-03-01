<p align="center"><img src="docs/readme-banner.png"/></p>

## The framework for agentic analytics

Onyx is an open-source framework for agentic analytics. It is declarative by design and written in Rust. Onyx is built with the following product principles in mind: open-source, performant, code-native, declarative, composable, and secure.

Agentic analytics applies software development lifecycle principles to AI-driven data analytics.
Just as traditional software follows a build-test-deploy pipeline, agentic analytics establishes a structured workflow for data agents, involving agent creation, prompt testing, and production deployment.

To learn more, read our [docs](https://docs.onyxint.ai).

### Quickstart

To install Onyx from binary, run the following command (Mac, Linux, WSL):

```bash
bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/onyx-hq/onyx/refs/heads/main/install_onyx.sh)
```

<details>
<summary>Alternative Installation Methods</summary>

#### Using Homebrew (macOS only)

```bash
brew install onyx-hq/onyx/onyx
```

#### Installing a Specific Version

```bash
ONYX_VERSION="0.1.0" bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/onyx-hq/onyx/refs/heads/main/install_onyx.sh)
```

</details>

To verify the installation, run:

```bash
onyx --version
```

And to initialize a repo, run the following in the directory you want to instantiate as an onyx repository:

```bash
onyx init
```

At this point, you should have a rudimentary onyx instance set up. To test this out, you can run the following commands from the root of the directory:

```bash
onyx run sql-generator.agent.yml "On how many nights did I get good sleep in the last year?"  # ask a question to the sample agent
onyx test sql-generator.agent.yml  # runs all defined tests against the sql-generator agent

onyx run report-generator.workflow.yml  # execute the sample workflow
onyx test report-generator.workflow.yml  # run all defined tests against the workflow
```

See our [docs](https://docs.onyxint.ai) on how to modify the agent file, seed it with context, run tests, and create workflows.
