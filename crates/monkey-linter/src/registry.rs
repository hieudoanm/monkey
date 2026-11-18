//! Rule registry: stable ids, descriptions, default severities, categories.

use monkey_core::Severity;

/// Coarse grouping for rules, used by future configuration and filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Language-agnostic source hygiene.
    General,
    /// Rules specific to Monkey C semantics.
    MonkeyC,
    /// Garmin / Connect IQ / watch-face aware rules.
    Garmin,
}

/// Static description of a lint rule.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// Stable kebab-case identifier reported in diagnostics.
    pub id: &'static str,
    pub description: &'static str,
    pub default_severity: Severity,
    pub category: Category,
}

/// All registered lint rules.
///
/// Lexical (`LEX*`) and syntax (`PARSE*`) diagnostics are produced by
/// `monkey-syntax`/`monkey-parser` and are not lint rules, so they are not
/// listed here.
pub const RULES: &[Rule] = &[
    Rule {
        id: "no-trailing-whitespace",
        description: "Trailing whitespace at end of line",
        default_severity: Severity::Warning,
        category: Category::General,
    },
    Rule {
        id: "no-multiple-empty-lines",
        description: "More than one consecutive blank line",
        default_severity: Severity::Warning,
        category: Category::General,
    },
    Rule {
        id: "use-space-indent",
        description: "Tab indentation; spaces are used instead",
        default_severity: Severity::Warning,
        category: Category::General,
    },
    Rule {
        id: "eof-newline",
        description: "File does not end with a newline",
        default_severity: Severity::Warning,
        category: Category::General,
    },
    Rule {
        id: "unreachable-code",
        description: "Statement after return/break/continue/throw in the same block",
        default_severity: Severity::Warning,
        category: Category::General,
    },
    Rule {
        id: "unused-variable",
        description: "Local variable declared but never referenced",
        default_severity: Severity::Warning,
        category: Category::General,
    },
    Rule {
        id: "identical-operands",
        description: "Both sides of a comparison are identical",
        default_severity: Severity::Warning,
        category: Category::General,
    },
    Rule {
        id: "empty-block",
        description: "Empty `{}` block",
        default_severity: Severity::Hint,
        category: Category::General,
    },
    Rule {
        id: "no-request-update-in-onupdate",
        description: "requestUpdate() inside onUpdate causes a continuous redraw loop",
        default_severity: Severity::Error,
        category: Category::Garmin,
    },
];

/// Look up a rule by id.
pub fn rule(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = RULES.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), RULES.len());
    }

    #[test]
    fn every_rule_is_findable() {
        for r in RULES {
            assert_eq!(rule(r.id).map(|x| x.id), Some(r.id));
        }
    }
}
