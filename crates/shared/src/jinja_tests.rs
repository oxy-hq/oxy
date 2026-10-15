use super::*;

fn env() -> Environment<'static> {
    let mut env = Environment::new();
    apply_limits(&mut env);
    add_now_function(&mut env);
    env
}

fn render(source: &str) -> Result<String, Error> {
    let env = env();
    let tmpl = env.template_from_str(source)?;
    render_bounded(&tmpl, minijinja::context! {})
}

/// `now(fmt=…)` with a format chrono rejects used to panic inside
/// `to_string()`; it is a render error now, and says which format.
#[test]
fn now_with_a_format_chrono_rejects_is_an_error_not_a_panic() {
    for source in ["{{ now(fmt='%Q') }}", "{{ now(utc=true, fmt='%Y-%Q') }}"] {
        let err = render(source).expect_err(source);
        assert!(
            err.to_string().contains("is not a strftime format"),
            "{source}: {err}"
        );
    }
}

#[test]
fn now_still_formats_a_valid_format() {
    assert_eq!(render("{{ now(fmt='%Y-%m-%d') }}").unwrap().len(), 10);
    assert!(render("{{ now(utc=true) }}").unwrap().contains('T'));
}

/// Loops nest, and `range` is capped per call, not per template: three
/// nested ranges are 10^15 iterations. Fuel stops them, and the error names
/// the limit.
#[test]
fn a_template_that_loops_without_end_stops_and_names_the_limit() {
    let source = "{% for a in range(100000) %}{% for b in range(100000) %}\
                  {% for c in range(100000) %}{% endfor %}{% endfor %}{% endfor %}";
    let err = render(source).expect_err("a 10^15-iteration loop must stop");
    let message = err.to_string();
    assert!(message.contains("TEMPLATE_FUEL"), "{message}");
    assert!(message.contains(&TEMPLATE_FUEL.to_string()), "{message}");
}

/// The output limit stops a render whose text passes MAX_RENDERED_BYTES, and
/// the error names it.
#[test]
fn a_render_past_the_output_limit_stops_and_names_the_limit() {
    let source = format!("{{{{ 'x' * {} }}}}", MAX_RENDERED_BYTES + 1);
    let message = render(&source)
        .expect_err("output past the cap")
        .to_string();
    assert!(message.contains("MAX_RENDERED_BYTES"), "{message}");
    // Exactly the limit is allowed.
    let at_limit = format!("{{{{ 'x' * {MAX_RENDERED_BYTES} }}}}");
    assert_eq!(render(&at_limit).unwrap().len(), MAX_RENDERED_BYTES);
}

#[test]
fn an_ordinary_loop_renders_under_the_limits() {
    let out = render("{% for i in range(10000) %}{{ i }},{% endfor %}").unwrap();
    assert!(out.starts_with("0,1,2,") && out.ends_with("9999,"));
}
