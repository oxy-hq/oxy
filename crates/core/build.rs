use shadow_rs::{BuildPattern, ShadowBuilder};

fn main() {
    ShadowBuilder::builder()
        .build_pattern(BuildPattern::Lazy)
        .build()
        .unwrap();
}
