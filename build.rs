use std::process::Command;
use std::fs;
use std::path::Path;


fn main() {
    /* In order to get pyo3 to use a particular virtual environment (and, in particular, for this
     * use case we want to use the virtual environment that contains the `onyx` installation), I
     * need to append the virtual environment's site-packages to sys path (see
     * https://github.com/PyO3/pyo3/discussions/3726).
     *
     * We could hard code this into the rust code to ensure that these packages are read by pyo3
     * correctly, but unfortunately, the poetry environment is somewhat arbitrarily named, which we
     * can only determine at build-time. So the issue is that, for a given installation, we need to
     * run `poetry shell` and `poetry install`, but it's only possible to know where that virtual
     * environment lives after the fact. So we need to somehow save the poetry virtualenv location
     * AFTER the poetry virtual env is created, then pass this into the rust code to build.
     *
     * While it seems like a natural way to do this is to set environmental variables, my concern
     * with this method was that it seemed like it could cause problems during development, where
     * one might be developing on one part of the system at a time, rather than building in
     * sequence, in which case the environmental variable may be unavailable to cargo.
     * 
     * So I've come to a very strange solution here, where I'm writing the poetry virtualenv path
     * to a rust file that is then included in the build.
     */

    // Get Poetry environment path
    let output = Command::new("poetry")
        .args(&["env", "info", "--path"])
        .output()
        .expect("Failed to execute poetry command");

    println!("output: {:?}", output);

    let poetry_env_path = String::from_utf8(output.stdout)
        .expect("Invalid UTF-8 output from poetry command")
        .trim()
        .to_string();

    // Generate a Rust file with the Poetry path
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("poetry_path.rs");
    fs::write(
        &dest_path,
        format!("pub const POETRY_ENV_PATH: &str = \"{}\";", poetry_env_path)
    ).unwrap();

}
