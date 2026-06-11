//! Code-declared feature flag registry.
//!
//! Adding a flag: append a `FlagDef` entry below. Removing a flag: delete
//! the entry; any DB row with that key becomes stale and is filtered out
//! at cache load (see `cache::init`). Renaming: treat as remove + add.

pub struct FlagDef {
    pub key: &'static str,
    pub description: &'static str,
    pub default_enabled: bool,
}

pub static FLAGS: &[FlagDef] = &[
    FlagDef {
        key: "billing",
        description: "Master switch for billing & subscription enforcement. \
                      When off, paywall and subscription guards are skipped \
                      for all orgs.",
        default_enabled: false,
    },
    FlagDef {
        key: "compile_boundary_disabled",
        description: "Kill switch for the compile boundary read path. When \
                      enabled, every hybrid reader short-circuits to FS, \
                      ignoring any promoted revisions and overriding \
                      `compile_runtime_use_postgres`. Use when a compile bug \
                      ships and reverting code is slower than flipping the \
                      runtime flag off.",
        default_enabled: false,
    },
    FlagDef {
        key: "compile_runtime_use_postgres",
        description: "Single Slice 4 master flag. When enabled, every runtime \
                      read path (app page load, procedure run, analytics agent \
                      execution, semantic query) resolves its definition from \
                      the compiled Postgres tables instead of walking the \
                      workspace filesystem. Replaces the earlier per-surface \
                      flags (compile_apps_use_postgres, _procedures_, _agents_, \
                      _semantic_) — those were over-engineered for our actual \
                      rollout pattern. The kill switch \
                      `compile_boundary_disabled` still wins; flip THIS flag to \
                      `true` to move every runtime read onto Postgres, flip the \
                      kill switch on if anything regresses.",
        default_enabled: false,
    },
];

pub fn get(key: &str) -> Option<&'static FlagDef> {
    FLAGS.iter().find(|f| f.key == key)
}

pub fn default_for(key: &str) -> bool {
    get(key).map(|f| f.default_enabled).unwrap_or(false)
}
