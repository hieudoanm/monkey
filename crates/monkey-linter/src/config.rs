//! Lint configuration: rule severity overrides and disables.
//!
//! Configured through `lint.*` keys in `monkey.jungle` (see
//! `monkey_project::lint_config`):
//!
//! ```text
//! lint.disable = unused-variable,empty-block
//! lint.severity.unused-variable = error
//! ```

use std::collections::{BTreeMap, BTreeSet};

use monkey_core::{Diagnostic, Severity};

/// Per-project lint configuration.
#[derive(Clone, Debug, Default)]
pub struct LintConfig {
    /// Rule ids to suppress entirely.
    pub disabled: BTreeSet<String>,
    /// Rule id → severity override.
    pub severity: BTreeMap<String, Severity>,
}

impl LintConfig {
    /// Apply config to a diagnostic list in place: drop disabled rules and
    /// rewrite severities.
    pub fn apply(&self, diags: &mut Vec<Diagnostic>) {
        if self.disabled.is_empty() && self.severity.is_empty() {
            return;
        }
        diags.retain(|d| !self.disabled.contains(d.code.as_str()));
        for d in diags.iter_mut() {
            if let Some(s) = self.severity.get(d.code.as_str()) {
                d.severity = *s;
            }
        }
    }

    /// Build a config from `monkey.jungle` entries (`key = value` pairs):
    ///
    /// - `lint.disable = id1,id2` — comma-separated rule ids to disable
    /// - `lint.severity.<id> = error|warning|information|hint` — severity
    ///   override for one rule
    pub fn from_jungle<'a>(entries: impl Iterator<Item = (&'a str, &'a str)>) -> LintConfig {
        let mut cfg = LintConfig::default();
        for (k, v) in entries {
            if k == "lint.disable" {
                for id in v.split(',') {
                    let id = id.trim();
                    if !id.is_empty() {
                        cfg.disabled.insert(id.to_string());
                    }
                }
            } else if let Some(id) = k.strip_prefix("lint.severity.") {
                if let Some(sev) = LintConfig::parse_severity(v) {
                    cfg.severity.insert(id.to_string(), sev);
                }
            }
        }
        cfg
    }

    /// Parse a severity name (`error`, `warning`, `information`/`info`,
    /// `hint`).
    pub fn parse_severity(s: &str) -> Option<Severity> {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Severity::Error),
            "warning" | "warn" => Some(Severity::Warning),
            "information" | "info" | "note" => Some(Severity::Information),
            "hint" => Some(Severity::Hint),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monkey_core::Span;

    fn diag(code: &str, sev: Severity) -> Diagnostic {
        Diagnostic {
            code: code.to_string(),
            severity: sev,
            message: code.to_string(),
            span: Some(Span::new(0, 1)),
            notes: vec![],
            help: None,
            suggestions: vec![],
        }
    }

    #[test]
    fn disables_and_overrides() {
        let mut cfg = LintConfig::default();
        cfg.disabled.insert("empty-block".to_string());
        cfg.severity
            .insert("unused-variable".to_string(), Severity::Error);
        let mut diags = vec![
            diag("empty-block", Severity::Hint),
            diag("unused-variable", Severity::Warning),
            diag("no-trailing-whitespace", Severity::Warning),
        ];
        cfg.apply(&mut diags);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].code, "unused-variable");
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[1].severity, Severity::Warning);
    }

    #[test]
    fn from_jungle_entries() {
        let entries = [
            ("lint.disable", "unused-variable, empty-block"),
            ("lint.severity.no-trailing-whitespace", "error"),
            ("project.manifest", "manifest.xml"),
        ];
        let cfg = LintConfig::from_jungle(entries.iter().copied());
        assert!(cfg.disabled.contains("unused-variable"));
        assert!(cfg.disabled.contains("empty-block"));
        assert_eq!(cfg.severity["no-trailing-whitespace"], Severity::Error);
    }
}
