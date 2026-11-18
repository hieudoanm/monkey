//! Integration tests for the Monkey C parser: fixtures, AST shape,
//! diagnostics, error recovery, and the visitor.

use std::path::PathBuf;

use monkey_parser::ast::*;
use monkey_parser::visit::Visitor;
use monkey_parser::{parse, ParseResult};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
}

fn parse_fixture(name: &str) -> ParseResult {
    parse(&fixture(name))
}

fn classes(result: &ParseResult) -> Vec<&ClassDecl> {
    result
        .file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Class(c) => Some(c),
            _ => None,
        })
        .collect()
}

fn functions(class: &ClassDecl) -> Vec<&FunctionDecl> {
    class
        .members
        .iter()
        .filter_map(|m| match m {
            ClassMember::Function(f) => Some(f),
            _ => None,
        })
        .collect()
}

#[test]
fn watchface_fixture_parses_clean() {
    let r = parse_fixture("watchface.txt");
    assert!(
        r.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn misc_fixture_parses_clean() {
    let r = parse_fixture("misc.txt");
    assert!(
        r.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn using_decls_and_aliases() {
    let r = parse_fixture("watchface.txt");
    let usings: Vec<&UsingDecl> = r
        .file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Using(u) => Some(u),
            _ => None,
        })
        .collect();
    assert_eq!(usings.len(), 3);
    assert_eq!(
        usings[0]
            .path
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>(),
        ["Toybox", "Graphics"]
    );
    assert_eq!(
        usings[1].alias.as_ref().map(|a| a.text.as_str()),
        Some("Sys")
    );
}

#[test]
fn class_decl_shape() {
    let r = parse_fixture("watchface.txt");
    let cls = classes(&r);
    assert_eq!(cls.len(), 1);
    let c = cls[0];
    assert_eq!(c.name.text, "ExampleView");
    assert_eq!(c.annotations.len(), 1);
    assert_eq!(c.annotations[0].name.text, "background");
    let sup = c.superclass.as_ref().expect("superclass");
    assert_eq!(
        sup.path.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
        ["WatchUi", "WatchFace"]
    );

    // Members: 3 fields + 3 functions.
    let fields: Vec<&FieldDecl> = c
        .members
        .iter()
        .filter_map(|m| match m {
            ClassMember::Field(f) => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].name.text, "counter");
    assert!(fields[0].mutable);
    assert_eq!(fields[1].visibility, Some(Visibility::Private));
    assert!(!fields[2].mutable); // const

    let fns = functions(c);
    assert_eq!(fns.len(), 4);
    let on_update = fns.iter().find(|f| f.name.text == "onUpdate").unwrap();
    assert_eq!(on_update.params.len(), 1);
    assert_eq!(on_update.params[0].name.text, "dc");
    assert_eq!(
        on_update.params[0].type_ann.as_ref().map(|t| t
            .path
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>()),
        Some(vec!["Dc"])
    );
    assert!(on_update.return_type.is_some());
}

#[test]
fn statements_and_expressions() {
    let r = parse_fixture("watchface.txt");
    let c = classes(&r)[0];
    let on_update = functions(c)
        .into_iter()
        .find(|f| f.name.text == "onUpdate")
        .unwrap();

    // Collect top-level statement kinds in onUpdate.
    let kinds: Vec<&str> = on_update
        .body
        .stmts
        .iter()
        .map(|s| match s {
            Stmt::VarDecl { .. } => "var",
            Stmt::Expr { .. } => "expr",
            Stmt::If { .. } => "if",
            Stmt::While { .. } => "while",
            Stmt::For { .. } => "for",
            Stmt::DoWhile { .. } => "do-while",
            Stmt::Try { .. } => "try",
            Stmt::Switch { .. } => "switch",
            _ => "other",
        })
        .collect();
    for want in [
        "var", "expr", "if", "while", "for", "do-while", "try", "switch",
    ] {
        assert!(kinds.contains(&want), "missing stmt kind {}", want);
    }
}

#[test]
fn expression_shapes() {
    let src = r#"
class T {
  function f() as Void {
    var a = x + y * 2;
    var b = cond ? 1 : 2;
    var c = foo.bar(1, "s").baz[0];
    var d = new Lang.Number(4);
    var e = obj as Lang.Number;
    var g = [1, 2];
    var h = {"k": v};
    var i = -x + !flag + ~bits;
    var j = k++ + --m;
    var l = (a & b) | c ^ d << 2 >>> 1;
    var m = p instanceof Lang.Number;
    var n = q in table;
    var o = x = y += 1;
  }
}
"#;
    let r = parse(src);
    assert!(
        r.diagnostics.is_empty(),
        "diags: {:?}",
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let c = classes(&r)[0];
    let f = functions(c)[0];
    // 13 var decls (a,b,c,d,e,g,h,i,j,l,m,n,o).
    let decls: Vec<&VarDecl> = f
        .body
        .stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::VarDecl { decls, .. } => Some(&decls[0]),
            _ => None,
        })
        .collect();
    assert_eq!(decls.len(), 13);

    // Spot-check AST node kinds.
    let init = |i: usize| decls[i].init.as_ref().unwrap();
    assert!(matches!(init(0), Expr::Binary { op: BinOp::Add, .. }));
    assert!(matches!(init(1), Expr::Ternary { .. }));
    assert!(
        matches!(init(2), Expr::Member { .. })
            || matches!(init(2), Expr::Index { .. })
            || matches!(init(2), Expr::Call { .. })
    );
    assert!(matches!(init(3), Expr::New { .. }));
    assert!(matches!(init(4), Expr::Cast { .. }));
    assert!(matches!(init(5), Expr::Array { .. }));
    assert!(matches!(init(6), Expr::Dict { .. }));
    assert!(matches!(
        init(9),
        Expr::Binary {
            op: BinOp::BitOr,
            ..
        }
    ));
    assert!(matches!(
        init(10),
        Expr::Binary {
            op: BinOp::InstanceOf,
            ..
        }
    ));
    assert!(matches!(init(11), Expr::Binary { op: BinOp::In, .. }));
}

#[test]
fn path_expressions_collapse() {
    // `Graphics.COLOR_WHITE` should become Expr::Path, not nested Members.
    let src = "class T {\n  function f() as Void {\n    var c = Graphics.COLOR_WHITE;\n    var d = A.B.C.D;\n  }\n}\n";
    let r = parse(src);
    assert!(r.diagnostics.is_empty());
    let f = functions(classes(&r)[0])[0];
    let decls: Vec<&VarDecl> = f
        .body
        .stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::VarDecl { decls, .. } => Some(&decls[0]),
            _ => None,
        })
        .collect();
    for d in decls {
        match d.init.as_ref().unwrap() {
            Expr::Path { segments, .. } => assert!(segments.len() >= 2),
            other => panic!("expected Path, got {:?}", other),
        }
    }
}

#[test]
fn malformed_recovers_and_continues() {
    let r = parse_fixture("malformed.txt");
    assert!(!r.diagnostics.is_empty());
    assert!(r
        .diagnostics
        .iter()
        .all(|d| d.code == "PARSE001" || d.code.starts_with("LEX")));

    // Both classes and the function after the broken one must survive.
    let cls = classes(&r);
    assert_eq!(cls.len(), 2);
    let names: Vec<&str> = cls.iter().map(|c| c.name.text.as_str()).collect();
    assert_eq!(names, ["Broken", "Second"]);
    let broken_fns = functions(cls[0]);
    let fn_names: Vec<&str> = broken_fns.iter().map(|f| f.name.text.as_str()).collect();
    assert!(fn_names.contains(&"ok"));
    assert!(fn_names.contains(&"bad"));
    assert!(fn_names.contains(&"after"));
}

#[test]
fn empty_and_trivia_only_sources() {
    for src in ["", "\n\n  \n", "// just a comment\n", "/* block */"] {
        let r = parse(src);
        assert!(r.file.items.is_empty());
        assert!(
            r.diagnostics.is_empty(),
            "src={:?} diags={:?}",
            src,
            r.diagnostics
        );
    }
}

/// Test visitor that counts nodes while preserving default recursion.
struct Counter {
    exprs: usize,
    functions: usize,
    classes: usize,
}

impl Visitor for Counter {
    fn visit_class(&mut self, d: &ClassDecl) {
        self.classes += 1;
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        if let Some(s) = &d.superclass {
            self.visit_type(s);
        }
        for m in &d.members {
            self.visit_class_member(m);
        }
    }

    fn visit_function(&mut self, f: &FunctionDecl) {
        self.functions += 1;
        for a in &f.annotations {
            self.visit_annotation(a);
        }
        for p in &f.params {
            self.visit_param(p);
        }
        if let Some(t) = &f.return_type {
            self.visit_type(t);
        }
        self.visit_block(&f.body);
    }

    fn visit_expr(&mut self, e: &Expr) {
        self.exprs += 1;
        match e {
            Expr::Missing { .. }
            | Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::This { .. }
            | Expr::Super { .. }
            | Expr::Path { .. } => {}
            Expr::Call { callee, args, .. } => {
                self.visit_expr(callee);
                for a in args {
                    self.visit_expr(a);
                }
            }
            Expr::Member { object, .. } => self.visit_expr(object),
            Expr::Index { object, index, .. } => {
                self.visit_expr(object);
                self.visit_expr(index);
            }
            Expr::Unary { expr, .. } | Expr::Postfix { expr, .. } => self.visit_expr(expr),
            Expr::Binary { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            Expr::Assign { target, value, .. } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            Expr::Ternary {
                cond, then, else_, ..
            } => {
                self.visit_expr(cond);
                self.visit_expr(then);
                self.visit_expr(else_);
            }
            Expr::New { type_, args, .. } => {
                self.visit_type(type_);
                for a in args {
                    self.visit_expr(a);
                }
            }
            Expr::Cast { expr, type_, .. } => {
                self.visit_expr(expr);
                self.visit_type(type_);
            }
            Expr::Array { elems, .. } => {
                for el in elems {
                    self.visit_expr(el);
                }
            }
            Expr::Dict { entries, .. } => {
                for (k, v) in entries {
                    self.visit_expr(k);
                    self.visit_expr(v);
                }
            }
            Expr::Paren { expr, .. } => self.visit_expr(expr),
        }
    }
}

#[test]
fn visitor_walks_everything() {
    let r = parse_fixture("watchface.txt");
    let mut c = Counter {
        exprs: 0,
        functions: 0,
        classes: 0,
    };
    c.visit_source_file(&r.file);
    assert_eq!(c.classes, 1);
    assert_eq!(c.functions, 4);
    assert!(c.exprs > 50, "exprs={}", c.exprs);
}

#[test]
fn spans_are_within_source() {
    let src = fixture("watchface.txt");
    let r = parse(&src);
    // Reuse Counter's full expr walk but also validate each span.
    struct SpanCheck<'a> {
        src: &'a str,
    }
    impl<'a> Visitor for SpanCheck<'a> {
        fn visit_expr(&mut self, e: &Expr) {
            let s = e.span();
            assert!(
                (s.start as usize) < self.src.len() && (s.end as usize) <= self.src.len(),
                "expr span {:?} out of range",
                s
            );
            assert!(self.src.is_char_boundary(s.start as usize));
            assert!(self.src.is_char_boundary(s.end as usize));
            // Delegate to the counting visitor's recursion by re-walking
            // through a nested Counter is overkill; replicate via match.
            match e {
                Expr::Call { callee, args, .. } => {
                    self.visit_expr(callee);
                    for a in args {
                        self.visit_expr(a);
                    }
                }
                Expr::Binary { left, right, .. }
                | Expr::Assign {
                    target: left,
                    value: right,
                    ..
                } => {
                    self.visit_expr(left);
                    self.visit_expr(right);
                }
                Expr::Ternary {
                    cond, then, else_, ..
                } => {
                    self.visit_expr(cond);
                    self.visit_expr(then);
                    self.visit_expr(else_);
                }
                Expr::Member { object, .. } => self.visit_expr(object),
                Expr::Index { object, index, .. } => {
                    self.visit_expr(object);
                    self.visit_expr(index);
                }
                Expr::Unary { expr, .. }
                | Expr::Postfix { expr, .. }
                | Expr::Paren { expr, .. }
                | Expr::Cast { expr, .. } => {
                    self.visit_expr(expr);
                }
                Expr::Array { elems, .. } => {
                    for el in elems {
                        self.visit_expr(el);
                    }
                }
                Expr::Dict { entries, .. } => {
                    for (k, v) in entries {
                        self.visit_expr(k);
                        self.visit_expr(v);
                    }
                }
                Expr::New { args, .. } => {
                    for a in args {
                        self.visit_expr(a);
                    }
                }
                _ => {}
            }
        }
    }
    let mut v = SpanCheck { src: &src };
    v.visit_source_file(&r.file);
}
