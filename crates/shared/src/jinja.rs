//! What every server-side Jinja environment shares: the `now()` global and
//! the limits one render runs under.
//!
//! Two environments render templates an author wrote — the exec-runtime
//! `Renderer` in `oxy` (Data App control defaults, retrieval text, eval
//! prompts) and the automation engine's (`agentic-automation`: SQL, prompts,
//! formatters, conditions, HTTP parts). Both build their environment with
//! [`apply_limits`] and render through [`render_bounded`], so a template that
//! loops without end or writes without end stops with an error that names
//! the limit, instead of holding a thread or growing a buffer until the
//! process dies.
//!
//! What this does **not** bound: memory a single instruction allocates
//! before anything is written. `"x" * n` is one instruction that allocates
//! `n` bytes, and minijinja has no hook on its operators. The output limit
//! refuses that value when it is written, after the allocation.

use std::io;

use minijinja::{Environment, Error, ErrorKind, Template, value::Kwargs};
use serde::Serialize;

/// The most instructions one render (or one expression evaluation) may run.
///
/// minijinja charges one unit for most instructions; a loop iteration that
/// writes one value costs 5, one that writes literal text 4. So this allows
/// about a million iterations — past any template a person writes (a
/// formatter over a 10,000-row result with ten columns is about 500,000 units)
/// — and bounds the CPU one render can take. A `range` is capped at 100,000
/// items per call but loops nest, so without this a template could run for as
/// long as it liked. Measured in a debug build: 240,000 units took 0.2–0.6 s
/// on a loaded machine; a release build is many times faster.
pub const TEMPLATE_FUEL: u64 = 5_000_000;

/// The most bytes one render may produce: 16 MiB.
///
/// Rendered text is SQL, a prompt, a formatter's message, a path. Sixteen
/// mebibytes is several times the largest of those — an LLM's whole context
/// window is a few — and still a size one request can afford to hold.
pub const MAX_RENDERED_BYTES: usize = 16 * 1024 * 1024;

/// Set [`TEMPLATE_FUEL`] on `env`. Every render and expression evaluation in
/// it then stops once the budget is spent.
pub fn apply_limits(env: &mut Environment<'_>) {
    env.set_fuel(Some(TEMPLATE_FUEL));
}

/// Register `now(utc?, fmt?)`: the current time, RFC 3339 by default, or
/// formatted by a strftime `fmt`.
///
/// A `fmt` chrono does not understand (`%Q`) is a render error. It used to be
/// a panic: chrono reports a bad format only when the value is written, and
/// `to_string()` turns that report into a panic.
pub fn add_now_function(env: &mut Environment<'_>) {
    env.add_function("now", now);
}

fn now(kwargs: Kwargs) -> Result<String, Error> {
    use chrono::{Local, Utc};

    let utc = kwargs.get::<Option<bool>>("utc")?.unwrap_or(false);
    let fmt = kwargs.get::<Option<String>>("fmt")?;
    match (utc, fmt) {
        (true, None) => Ok(Utc::now().to_rfc3339()),
        (false, None) => Ok(Local::now().to_rfc3339()),
        (true, Some(fmt)) => write_formatted(Utc::now().format(&fmt), &fmt),
        (false, Some(fmt)) => write_formatted(Local::now().format(&fmt), &fmt),
    }
}

/// Write a chrono `DelayedFormat` without the panic `to_string()` raises when
/// the format is invalid.
fn write_formatted(formatted: impl std::fmt::Display, fmt: &str) -> Result<String, Error> {
    use std::fmt::Write;

    let mut out = String::new();
    write!(out, "{formatted}").map_err(|_| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("now(): `{fmt}` is not a strftime format (see chrono's `format::strftime`)"),
        )
    })?;
    Ok(out)
}

/// Render `tmpl` with `ctx`, stopping at [`MAX_RENDERED_BYTES`] of output.
///
/// The fuel limit is the environment's ([`apply_limits`]); running out of
/// either is an error whose message names the limit ([`name_the_limit`]).
pub fn render_bounded<S: Serialize>(tmpl: &Template<'_, '_>, ctx: S) -> Result<String, Error> {
    let mut out = BoundedWriter::new(MAX_RENDERED_BYTES);
    match tmpl.render_captured_to(ctx, &mut out) {
        Ok(_) => Ok(String::from_utf8(out.buf)
            .map_err(|e| Error::new(ErrorKind::WriteFailure, e.to_string()))?),
        Err(_) if out.exceeded => Err(output_limit_error()),
        Err(err) => Err(name_the_limit(err)),
    }
}

/// `err`, with its message naming [`TEMPLATE_FUEL`] when that is what ran out.
///
/// minijinja's own message is "engine ran out of fuel", which tells an
/// author neither what the limit is nor what to change. Any other error is
/// returned as it is.
pub fn name_the_limit(err: Error) -> Error {
    if err.kind() != ErrorKind::OutOfFuel {
        return err;
    }
    Error::new(
        ErrorKind::InvalidOperation,
        format!(
            "the template was stopped after {TEMPLATE_FUEL} instructions, the most one render \
             may run (TEMPLATE_FUEL). A loop over a large range, or loops nested inside each \
             other, is the usual cause"
        ),
    )
}

fn output_limit_error() -> Error {
    Error::new(
        ErrorKind::InvalidOperation,
        format!(
            "the template's output passed {MAX_RENDERED_BYTES} bytes (16 MiB), the most one \
             render may produce (MAX_RENDERED_BYTES)"
        ),
    )
}

/// An `io::Write` that refuses a write taking it past `limit` bytes.
struct BoundedWriter {
    buf: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl BoundedWriter {
    fn new(limit: usize) -> Self {
        BoundedWriter {
            buf: Vec::new(),
            limit,
            exceeded: false,
        }
    }
}

impl io::Write for BoundedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.buf.len().saturating_add(data.len()) > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("rendered output limit reached"));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "jinja_tests.rs"]
mod tests;
