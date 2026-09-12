//! Syn walk for `*_used` method calls and nearby `use_!` purposes.

use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall, ExprPath, Item, Lit, Macro, Stmt};

use crate::DataUseScanError;

/// A single discovered `*_used` call with purpose text.
#[derive(Debug, Clone)]
pub struct FoundUse {
    pub purpose: String,
    pub file: String,
    pub line: u32,
    pub crate_name: String,
    pub receiver: String,
    pub method: String,
}

const USED_METHODS: &[&str] = &[
    "get_used",
    "create_used",
    "update_used",
    "delete_used",
    "delete_now_used",
    "upsert_used",
    "merge_used",
    "query_used",
    "get_mutable_used",
    "execute_used",
    "get_entity_used",
    "get_record_json_used",
    "get_record_used",
    "latest_ids_used",
    "get_by_composite_key_used",
    "upsert_by_composite_key_used",
];

/// Parse `path` and collect `*_used` call sites.
pub fn scan_file(
    path: &Path,
    crate_name: &str,
    rel_file: &str,
) -> Result<Vec<FoundUse>, DataUseScanError> {
    let src = std::fs::read_to_string(path).map_err(|e| DataUseScanError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let file = syn::parse_file(&src).map_err(|e| DataUseScanError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let mut visitor = UseVisitor {
        crate_name: crate_name.to_string(),
        file: rel_file.to_string(),
        source: src,
        hits: Vec::new(),
    };
    visitor.visit_file(&file);
    Ok(visitor.hits)
}

struct UseVisitor {
    crate_name: String,
    file: String,
    source: String,
    hits: Vec<FoundUse>,
}

impl<'ast> Visit<'ast> for UseVisitor {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method = node.method.to_string();
        if USED_METHODS.contains(&method.as_str()) {
            if let Some(purpose) = extract_purpose_from_args(&node.args) {
                let receiver = expr_type_name(&node.receiver);
                let line = line_for_span(&self.source, node.span());
                self.hits.push(FoundUse {
                    purpose,
                    file: self.file.clone(),
                    line,
                    crate_name: self.crate_name.clone(),
                    receiver,
                    method,
                });
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Some((receiver, method)) = path_call_receiver_method(&node.func) {
            if USED_METHODS.contains(&method.as_str()) {
                if let Some(purpose) = extract_purpose_from_args(&node.args) {
                    let line = line_for_span(&self.source, node.span());
                    self.hits.push(FoundUse {
                        purpose,
                        file: self.file.clone(),
                        line,
                        crate_name: self.crate_name.clone(),
                        receiver,
                        method,
                    });
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_stmt(&mut self, node: &'ast Stmt) {
        syn::visit::visit_stmt(self, node);
    }

    fn visit_item(&mut self, node: &'ast Item) {
        syn::visit::visit_item(self, node);
    }
}

fn path_call_receiver_method(func: &Expr) -> Option<(String, String)> {
    let Expr::Path(ExprPath { path, .. }) = func else {
        return None;
    };
    let mut segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let method = segments.pop()?;
    let receiver = if segments.is_empty() {
        String::new()
    } else {
        segments.join("::")
    };
    Some((receiver, method))
}

fn expr_type_name(expr: &Expr) -> String {
    match expr {
        Expr::Path(p) => p
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Expr::Paren(p) => expr_type_name(&p.expr),
        Expr::Group(g) => expr_type_name(&g.expr),
        Expr::Try(t) => expr_type_name(&t.expr),
        Expr::Await(a) => expr_type_name(&a.base),
        Expr::Field(f) => expr_type_name(&f.base),
        Expr::Reference(r) => expr_type_name(&r.expr),
        Expr::Unary(u) => expr_type_name(&u.expr),
        _ => String::new(),
    }
}

fn extract_purpose_from_args(
    args: &syn::punctuated::Punctuated<Expr, syn::token::Comma>,
) -> Option<String> {
    for arg in args {
        if let Some(purpose) = extract_purpose_from_expr(arg) {
            return Some(purpose);
        }
    }
    None
}

fn extract_purpose_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Macro(m) => extract_purpose_from_macro(&m.mac),
        Expr::Call(c) => {
            // DataUsePurpose::new("…", file!(), line!()) — rare; still accept string lit first arg.
            if let Expr::Path(p) = &*c.func {
                let name = p
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                if name == "new" || name == "DataUsePurpose" {
                    if let Some(Expr::Lit(lit)) = c.args.first() {
                        if let Lit::Str(s) = &lit.lit {
                            return Some(trim_purpose(s.value()));
                        }
                    }
                }
            }
            None
        }
        Expr::Paren(p) => extract_purpose_from_expr(&p.expr),
        Expr::Group(g) => extract_purpose_from_expr(&g.expr),
        _ => None,
    }
}

fn extract_purpose_from_macro(mac: &Macro) -> Option<String> {
    let last = mac.path.segments.last()?.ident.to_string();
    if last != "use_" && last != "use" {
        return None;
    }
    // `use_!("…")` or `use_!(r#"…"#)`
    let tokens = mac.tokens.clone();
    if let Ok(Lit::Str(s)) = syn::parse2::<Lit>(tokens.clone()) {
        return Some(trim_purpose(s.value()));
    }
    // Fallback: treat token stream as a string literal via syn::Expr
    if let Ok(Expr::Lit(expr_lit)) = syn::parse2::<Expr>(tokens) {
        if let Lit::Str(s) = expr_lit.lit {
            return Some(trim_purpose(s.value()));
        }
    }
    None
}

fn trim_purpose(s: String) -> String {
    // Normalize leading/trailing blank lines from raw string blocks; keep inner markdown.
    let lines: Vec<&str> = s.lines().collect();
    if lines.is_empty() {
        return s;
    }
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    // Dedent by common leading whitespace of non-empty lines.
    let body = &lines[start..end];
    let min_indent = body
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').count())
        .min()
        .unwrap_or(0);
    body.iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                l.chars().skip(min_indent).collect::<String>()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_for_span(source: &str, span: proc_macro2::Span) -> u32 {
    // syn spans in non-proc-macro context often lack precise line info; fall back to
    // searching the start byte via unstable API when available, else 1.
    let start = span.start();
    if start.line > 0 {
        return start.line as u32;
    }
    // Last resort: count newlines up to a needle from Display (unreliable) — keep 1.
    let _ = source;
    1
}
