use std::fs;
use std::path::Path;

use globset::{GlobBuilder, GlobMatcher};

pub(crate) struct IgnoreRules {
    patterns: Vec<(GlobMatcher, bool)>, // (matcher, negated)
}

impl IgnoreRules {
    pub(crate) fn load(origin_dir: &Path) -> Self {
        let ignore_path = origin_dir.join(".dotsyncignore");
        let mut patterns = Vec::new();

        let Ok(content) = fs::read_to_string(&ignore_path) else {
            return Self { patterns };
        };

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (raw, negated) = if let Some(p) = line.strip_prefix('!') {
                (p, true)
            } else {
                (line, false)
            };

            let normalized = normalize_pattern(raw);
            match GlobBuilder::new(&normalized).literal_separator(true).build() {
                Ok(glob) => patterns.push((glob.compile_matcher(), negated)),
                Err(e) => eprintln!(
                    "Warning: invalid pattern '{raw}' in .dotsyncignore: {e}"
                ),
            }
        }

        Self { patterns }
    }

    /// Returns true if `relative_path` matches any active ignore pattern.
    /// Patterns are evaluated in order; last match wins (gitignore semantics).
    pub(crate) fn is_ignored(&self, relative_path: &Path) -> bool {
        let mut ignored = false;
        for (matcher, negated) in &self.patterns {
            if matcher.is_match(relative_path) {
                ignored = !negated;
            }
        }
        ignored
    }
}

/// Translates a raw pattern line into a globset-compatible glob:
///
/// - No `/` → matches name at any depth           → prepend `**/`
/// - Leading `/` → anchored to root               → strip leading `/`
/// - Already starts with `**/` → leave unchanged
/// - Otherwise → path relative to root, leave unchanged
fn normalize_pattern(pattern: &str) -> String {
    let p = pattern.trim_end_matches('/');
    if p.starts_with("**/") {
        p.to_string()
    } else if let Some(rest) = p.strip_prefix('/') {
        rest.to_string()
    } else if !p.contains('/') {
        format!("**/{p}")
    } else {
        p.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(pattern: &str) -> GlobMatcher {
        GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .unwrap()
            .compile_matcher()
    }

    fn matches(pattern: &str, path: &str) -> bool {
        build(&normalize_pattern(pattern)).is_match(path)
    }

    #[test]
    fn no_slash_matches_at_any_depth() {
        assert!(matches(".DS_Store", ".DS_Store"));
        assert!(matches(".DS_Store", ".config/nvim/.DS_Store"));
        assert!(matches("*.log", "debug.log"));
        assert!(matches("*.log", ".config/app/debug.log"));
    }

    #[test]
    fn path_with_slash_anchored_to_root() {
        assert!(matches(".config/nvim", ".config/nvim"));
        assert!(!matches(".config/nvim", ".local/.config/nvim"));
    }

    #[test]
    fn leading_slash_anchored_to_root() {
        assert!(matches("/.zshrc", ".zshrc"));
        assert!(!matches("/.zshrc", "home/.zshrc"));
    }

    #[test]
    fn double_star_passthrough() {
        assert!(matches("**/.DS_Store", ".DS_Store"));
        assert!(matches("**/.DS_Store", "a/b/.DS_Store"));
    }

    #[test]
    fn trailing_slash_stripped() {
        assert!(matches("sessions/", ".config/nvim/sessions"));
        assert!(matches("sessions/", "sessions"));
    }

    #[test]
    fn negation_order_last_wins() {
        let patterns = vec![
            (build("**/*.log"), false),
            (build("**/keep.log"), true),
        ];
        let rules = IgnoreRules { patterns };
        assert!(rules.is_ignored(std::path::Path::new("debug.log")));
        assert!(!rules.is_ignored(std::path::Path::new("keep.log")));
    }

    #[test]
    fn star_does_not_cross_separator() {
        // .claude/* must match direct children only
        assert!(matches(".claude/*", ".claude/agents"));
        assert!(matches(".claude/*", ".claude/settings.json"));
        assert!(!matches(".claude/*", ".claude/agents/skill.md"));

        // negation glob must match the directory exactly
        assert!(matches(".claude/agents/", ".claude/agents"));
        assert!(!matches(".claude/agents/", ".claude/agents/skill.md"));
    }

    #[test]
    fn full_ignore_rules_scenario() {
        // Simulates: .claude/* + !.claude/agents/ + !.claude/skills/
        let patterns = vec![
            (build(".claude/*"), false),
            (build(".claude/agents"), true),
            (build(".claude/skills"), true),
        ];
        let rules = IgnoreRules { patterns };

        // Direct children that are NOT negated → ignored
        assert!(rules.is_ignored(std::path::Path::new(".claude/settings.json")));
        assert!(rules.is_ignored(std::path::Path::new(".claude/hooks")));
        // Negated directories → NOT ignored
        assert!(!rules.is_ignored(std::path::Path::new(".claude/agents")));
        assert!(!rules.is_ignored(std::path::Path::new(".claude/skills")));
        // Files inside negated dirs → NOT ignored (star doesn't reach them)
        assert!(!rules.is_ignored(std::path::Path::new(".claude/agents/skill.md")));
        assert!(!rules.is_ignored(std::path::Path::new(".claude/skills/foo.md")));
    }
}
