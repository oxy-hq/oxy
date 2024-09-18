# onyx cli

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

The build sequencing is as follows:
- The python modules are installed using `poetry` to a virtual environment.
- The rust crate is built, and uses `pyo3` to execute the code *using the virtual environment that was made in the previous step*.

### Repository structure
This repository is a mixed Python/rust repository.
- ./onyx contains the backend-workspaces code (directly from titanium)
- ./src contains rust code

The CLI tool is built in Rust, and executes code from the Python backend code with `pyo3`. The choice of rust for the CLI tool primarily because the CLI is faster, and longer-term, we will want to extend the capabilities of the CLI to do things that are latency-sensitive, e.g. fuzzy-searching through command history, exploring results, viewing warehouse context.

