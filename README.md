<p align="center"><img src="docs/readme-banner.png"/></p>

<p align="center">
  <a href="https://github.com/onyx-hq/onyx/actions/workflows/ci.yaml">
    <img src="https://github.com/onyx-hq/onyx/actions/workflows/ci.yaml/badge.svg" alt="CI Status">
  </a>
</p>

## The framework for agentic analytics

Onyx is a lightweight, declarative framework for agentic analytics. Agentic analytics applies software development lifecycle principles to AI-driven data analysis. Just as traditional software follows a build-test-deploy pipeline, agentic analytics establishes a structured workflow for AI agents, involving agent creation, prompt testing, and production deployment.

To learn more, read our [docs](https://docs.onyxint.ai).

### Quickstart

To install Onyx from binary, run the following command (Mac, Linux, WSL):

```bash
bash <(curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/onyx-hq/onyx-public-releases/refs/heads/main/install_onyx.sh)
```

And to initialize a repo, run the following in the directory you want to instantiate as an onyx repository:

```bash
onyx init
```

At this point, you should have a rudimentary onyx instance set up. To test this out, you can run the following from the root of the directory:

```bash
onyx run sql-generator.agent.yml "what is the capital of France?"
```

See our [docs](https://docs.onyxint.ai) on how to modify the agent file, seed it with context, run tests, and create workflows.
