//! Lint diagnostics for Monkey C source.
//!
//! Rules are registered in [`registry::RULES`]. Text rules also apply to
//! `.jungle` files; Monkey C rules run on the parsed AST and include
//! lexical (`LEX*`) and syntax (`PARSE*`) diagnostics from the parser.

mod ast;
mod registry;
mod suppression;
mod text;

pub use monkey_core::Diagnostic;
pub use registry::{rule, Category, Rule, RULES};

use monkey_parser::ast::SourceFile;
use monkey_parser::parse;

/// Run all lint rules over Monkey C source. `path` is used for context only.
///
/// Includes text rules, lexical/syntax diagnostics, and AST rules.
pub fn lint_monkeyc(src: &str, _path: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    lint_text(src, &mut diags);
    let parsed = parse(src);
    // Lexical + parse diagnostics produced by the shared pipeline.
    diags.extend(parsed.diagnostics);
    ast::lint_ast(src, &parsed.file, &mut diags);
    suppression::filter(src, &mut diags, 0);
    diags
}

/// Lint an already-parsed file (skips re-parsing; caller supplies src for
/// text rules and AST rules).
pub fn lint_parsed(src: &str, file: &SourceFile, syntax_diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    lint_text(src, &mut diags);
    diags.extend(syntax_diags);
    ast::lint_ast(src, file, &mut diags);
    suppression::filter(src, &mut diags, 0);
    diags
}

/// Text-level rules that also apply to Jungle files.
pub fn lint_text(src: &str, diags: &mut Vec<Diagnostic>) {
    let since = diags.len();
    text::lint_text(src, diags);
    suppression::filter(src, diags, since);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_trailing_whitespace() {
        let d = lint_monkeyc("var x = 1;   \nvar y = 2;\n", "t.mc");
        assert!(d.iter().any(|x| x.code == "no-trailing-whitespace"));
    }

    #[test]
    fn flags_missing_eof_newline() {
        let d = lint_monkeyc("var x = 1;", "t.mc");
        assert!(d.iter().any(|x| x.code == "eof-newline"));
    }

    #[test]
    fn flags_request_update_in_onupdate() {
        let src = "class V extends WatchUi.WatchFace {\n  function onUpdate(dc as Dc) as Void {\n    WatchUi.requestUpdate();\n  }\n  function onShow() as Void {\n    WatchUi.requestUpdate(WatchUi.UPDATE_EVERY_MINUTE);\n  }\n}\n";
        let d = lint_monkeyc(src, "t.mc");
        let hits: Vec<_> = d
            .iter()
            .filter(|x| x.code == "no-request-update-in-onupdate")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "diags: {:?}",
            d.iter().map(|x| &x.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, monkey_core::Severity::Error);
        // Line 3 via span.
        let map = monkey_core::SourceMap::new(src);
        let (line, _) = map.line_col(hits[0].span.unwrap().start, src);
        assert_eq!(line, 3);
    }

    #[test]
    fn flags_unused_variable() {
        let src = "class T {\n  function f() as Void {\n    var used = 1;\n    var unused = 2;\n    Sys.println(used);\n  }\n}\n";
        let d = lint_monkeyc(src, "t.mc");
        let hits: Vec<_> = d.iter().filter(|x| x.code == "unused-variable").collect();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].message.contains("unused"));
    }

    #[test]
    fn does_not_flag_used_or_shadowed_vars() {
        // `counter` is a field used bare inside methods — must not flag.
        let src = "class T {\n  var counter = 0;\n  function f() as Void {\n    counter += 1;\n    var local = counter;\n    Sys.println(local);\n  }\n}\n";
        let d = lint_monkeyc(src, "t.mc");
        assert!(!d.iter().any(|x| x.code == "unused-variable"));
    }

    #[test]
    fn flags_unreachable_code() {
        let src = "class T {\n  function f() as Void {\n    return;\n    var x = 1;\n    Sys.println(x);\n  }\n}\n";
        let d = lint_monkeyc(src, "t.mc");
        let hits: Vec<_> = d.iter().filter(|x| x.code == "unreachable-code").collect();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn flags_identical_operands() {
        let src = "class T {\n  function f(x as Lang.Number) as Void {\n    if (x == x) {\n      Sys.println(1);\n    }\n  }\n}\n";
        let d = lint_monkeyc(src, "t.mc");
        assert!(d.iter().any(|x| x.code == "identical-operands"));
    }

    #[test]
    fn reports_parse_errors() {
        let d = lint_monkeyc("class T {\n  function f( {\n}\n", "t.mc");
        assert!(d.iter().any(|x| x.code == "PARSE001"));
    }

    #[test]
    fn clean_file_has_no_diags() {
        let src = "class Foo {\n  function bar() as Void {\n    var x = 1;\n    Sys.println(x);\n  }\n}\n";
        assert!(lint_monkeyc(src, "t.mc").is_empty());
    }
}
