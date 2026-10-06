//! Datalog subset to SQL (design §8). Each attribute is a binary relation over the index tables;
//! variables become join columns, constants become parameters, predicates become SQL
//! expressions. Anything outside the subset fails with [`QueryError::Unsupported`].

use std::collections::HashMap;

use rusqlite::types::Value;

use super::dates;
use super::dsl::{Query, lower_nfc, parse_prop_value};
use super::edn::Edn;
use super::{QueryContext, QueryError};

type R<T> = Result<T, QueryError>;

fn unsupported<T>(what: impl Into<String>) -> R<T> {
    Err(QueryError::Unsupported(what.into()))
}

fn syntax<T>(what: impl Into<String>) -> R<T> {
    Err(QueryError::Syntax(what.into()))
}

/// Entity types of the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    /// A row of `blocks`.
    Block,
    /// A row of `pages`.
    Page,
    /// A row of `files`.
    File,
}

impl Ty {
    fn table(self) -> &'static str {
        match self {
            Ty::Block => "blocks",
            Ty::Page => "pages",
            Ty::File => "files",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Ty::Block => "a block",
            Ty::Page => "a page",
            Ty::File => "a file",
        }
    }
}

/// SQL text with its positional parameters.
#[derive(Debug, Clone, Default)]
pub(super) struct S {
    sql: String,
    params: Vec<Value>,
}

impl S {
    fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            params: Vec::new(),
        }
    }

    fn param(v: Value) -> Self {
        Self {
            sql: "?".to_owned(),
            params: vec![v],
        }
    }

    fn then(mut self, t: &str) -> Self {
        self.sql.push_str(t);
        self
    }

    fn with(mut self, o: &S) -> Self {
        self.sql.push_str(&o.sql);
        self.params.extend(o.params.iter().cloned());
        self
    }
}

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

/// A scalar SQL value with its textual and (optionally) numeric view.
#[derive(Debug, Clone)]
pub(super) struct Val {
    text: S,
    num: Option<S>,
    /// Text is case-folded (`value_norm`): compare against folded constants.
    ci: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vk {
    Text,
    Num,
    Bool,
}

impl Val {
    fn from_col(col: &str, vk: Vk) -> Self {
        match vk {
            Vk::Text => Val {
                text: S::new(col),
                num: None,
                ci: false,
            },
            Vk::Num => Val {
                text: S::new(format!("CAST({col} AS TEXT)")),
                num: Some(S::new(col)),
                ci: false,
            },
            Vk::Bool => Val {
                text: S::new(format!("(CASE {col} WHEN 1 THEN 'true' ELSE 'false' END)")),
                num: Some(S::new(col)),
                ci: false,
            },
        }
    }

    /// The expression used to report the value.
    fn output(&self) -> S {
        match &self.num {
            Some(n) => S::new("COALESCE(")
                .with(n)
                .then(", ")
                .with(&self.text)
                .then(")"),
            None => self.text.clone(),
        }
    }
}

#[derive(Debug, Clone)]
enum Binding {
    Entity {
        ty: Ty,
        id: S,
        alias: Option<String>,
    },
    Val(Val),
    Const(Edn),
    Props {
        owner: String,
    },
}

#[derive(Debug, Clone, Default)]
pub(super) struct Scope {
    from: Vec<String>,
    wh: Vec<S>,
    vars: HashMap<String, Binding>,
}

impl Scope {
    fn child(&self) -> Scope {
        Scope {
            from: Vec::new(),
            wh: Vec::new(),
            vars: self.vars.clone(),
        }
    }

    /// `SELECT 1 FROM ... WHERE ...` of this scope.
    fn exists_body(&self) -> S {
        let mut s = S::new("SELECT 1");
        if !self.from.is_empty() {
            s = s.then(" FROM ").then(&self.from.join(", "));
        }
        if !self.wh.is_empty() {
            s = s.then(" WHERE ");
            for (i, w) in self.wh.iter().enumerate() {
                if i > 0 {
                    s = s.then(" AND ");
                }
                s = s.with(w);
            }
        }
        s
    }
}

/// What one result column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColKind {
    /// A block id, hydrated by the caller.
    Block,
    /// A page id, hydrated by the caller.
    Page,
    /// A file path.
    File,
    /// A scalar.
    Value,
}

/// Shape of the `:find` spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `:find ?a ?b`: a set of tuples.
    Relation,
    /// `:find [?a ...]`
    Collection,
    /// `:find [?a ?b]`
    Tuple,
    /// `:find ?a .`
    Scalar,
}

/// Output expressions, column kinds, names, aggregate functions and shape of a `:find`.
pub(super) type FindSpec = (
    Vec<S>,
    Vec<ColKind>,
    Vec<String>,
    Vec<Option<&'static str>>,
    Shape,
);

pub(super) struct Comp<'a> {
    ctx: &'a QueryContext,
    types: HashMap<String, Ty>,
    n: usize,
    warnings: Vec<String>,
}

fn var_name(e: &Edn) -> Option<&str> {
    match e {
        Edn::Sym(s) if s.starts_with('?') => Some(s),
        _ => None,
    }
}

fn is_wildcard(e: &Edn) -> bool {
    matches!(e, Edn::Sym(s) if s == "_")
}

fn attr_name(e: &Edn) -> R<String> {
    match e {
        Edn::Kw(k) => {
            let k = match k.strip_prefix("page/") {
                Some(rest) => format!("block/{rest}"),
                None => k.clone(),
            };
            let k = if k == "block/ref-pages" {
                "block/refs".to_owned()
            } else {
                k
            };
            if k.contains("/_") {
                return unsupported(format!("reverse attribute :{k}"));
            }
            Ok(k)
        }
        other => unsupported(format!("attribute position {other}")),
    }
}

fn norm_key(k: &str) -> String {
    k.trim().to_lowercase().replace(['_', ' '], "-")
}

fn const_value(c: &Edn) -> R<Value> {
    Ok(match c {
        Edn::Str(s) => text(s),
        Edn::Int(n) => Value::Integer(*n),
        Edn::Float(x) => Value::Real(*x),
        Edn::Bool(b) => Value::Integer(i64::from(*b)),
        other => return unsupported(format!("constant {other}")),
    })
}

/// Page key of a name argument (`"[[Foo]]"` or `"foo"`).
fn page_key_of(c: &Edn) -> R<String> {
    match c {
        Edn::Str(s) | Edn::Sym(s) | Edn::Kw(s) => {
            let t = s.trim();
            let t = t
                .strip_prefix("[[")
                .and_then(|r| r.strip_suffix("]]"))
                .unwrap_or(t);
            Ok(bitacora_core::naming::page_key(t.trim()))
        }
        other => syntax(format!("expected a page name, got {other}")),
    }
}

fn str_set(c: &Edn) -> R<Vec<String>> {
    match c {
        Edn::Set(v) | Edn::Vector(v) | Edn::List(v) => v
            .iter()
            .map(|e| match e {
                Edn::Str(s) | Edn::Sym(s) | Edn::Kw(s) => Ok(s.clone()),
                other => syntax(format!("expected a name, got {other}")),
            })
            .collect(),
        Edn::Str(s) | Edn::Sym(s) | Edn::Kw(s) => Ok(vec![s.clone()]),
        other => syntax(format!("expected a set of names, got {other}")),
    }
}

/// Column of an attribute that holds a scalar: `(column, kind, nullable)`.
fn column(ty: Ty, a: &str, attr: &str) -> Option<(String, Vk, bool)> {
    let c = |col: &str, vk, nullable| Some((format!("{a}.{col}"), vk, nullable));
    match (ty, attr) {
        (Ty::Page, "block/name") => c("name", Vk::Text, false),
        (Ty::Page, "block/original-name") => c("original_name", Vk::Text, false),
        (Ty::Page, "block/journal?") => c("is_journal", Vk::Bool, false),
        (Ty::Page, "block/journal-day") => c("journal_day", Vk::Num, true),
        (Ty::Page, "block/uuid") => c("uuid", Vk::Text, false),
        (Ty::Page, "block/format") => c("format", Vk::Text, true),
        (Ty::Page, "block/created-at") => c("created_at", Vk::Num, true),
        (Ty::Page, "block/updated-at") => c("updated_at", Vk::Num, true),
        (Ty::Block, "block/uuid") => c("uuid", Vk::Text, false),
        (Ty::Block, "block/content") => c("content", Vk::Text, false),
        (Ty::Block, "block/format") => c("format", Vk::Text, false),
        (Ty::Block, "block/marker") => c("marker", Vk::Text, true),
        (Ty::Block, "block/priority") => c("priority", Vk::Text, true),
        (Ty::Block, "block/scheduled") => c("scheduled", Vk::Num, true),
        (Ty::Block, "block/deadline") => c("deadline", Vk::Num, true),
        (Ty::Block, "block/repeated?") => c("repeated", Vk::Bool, false),
        (Ty::Block, "block/collapsed?") => c("collapsed", Vk::Bool, false),
        (Ty::Block, "block/pre-block?") => c("is_pre_block", Vk::Bool, false),
        (Ty::Block, "block/heading") => c("heading", Vk::Num, true),
        (Ty::Block, "block/created-at") => c("created_at", Vk::Num, true),
        (Ty::Block, "block/updated-at") => c("updated_at", Vk::Num, true),
        (Ty::File, "file/path") => c("path", Vk::Text, false),
        _ => None,
    }
}

const PAGE_ONLY: &[&str] = &[
    "block/name",
    "block/original-name",
    "block/journal?",
    "block/journal-day",
    "block/namespace",
    "block/alias",
    "block/tags",
    "block/file",
];
const BLOCK_ONLY: &[&str] = &[
    "block/content",
    "block/marker",
    "block/priority",
    "block/scheduled",
    "block/deadline",
    "block/repeated?",
    "block/collapsed?",
    "block/pre-block?",
    "block/heading",
    "block/page",
    "block/parent",
    "block/refs",
    "block/path-refs",
];

/// Entity type of the value of an attribute, when it is known from the attribute alone.
fn value_ty(attr: &str) -> Option<Ty> {
    match attr {
        "block/namespace" | "block/alias" | "block/tags" | "block/page" | "block/path-refs" => {
            Some(Ty::Page)
        }
        "block/file" => Some(Ty::File),
        _ => None,
    }
}

fn rule_entity_ty(rule: &str) -> Option<Ty> {
    match rule {
        "page-ref" | "block-content" | "task" | "priority" | "property" | "has-property"
        | "page" | "between" => Some(Ty::Block),
        "page-property" | "has-page-property" | "namespace" | "page-tags" | "all-page-tags" => {
            Some(Ty::Page)
        }
        _ => None,
    }
}

const RULES: &[&str] = &[
    "page-ref",
    "block-content",
    "task",
    "priority",
    "property",
    "has-property",
    "page",
    "between",
    "page-property",
    "has-page-property",
    "namespace",
    "page-tags",
    "all-page-tags",
];

const EITHER: &[&str] = &[
    "block/uuid",
    "block/format",
    "block/created-at",
    "block/updated-at",
    "block/properties",
];

fn known_attr(attr: &str) -> bool {
    PAGE_ONLY.contains(&attr)
        || BLOCK_ONLY.contains(&attr)
        || EITHER.contains(&attr)
        || attr == "file/path"
}

fn collect_vars(e: &Edn, out: &mut Vec<String>) {
    match e {
        Edn::Sym(s) if s.starts_with('?') => {
            if !out.contains(s) {
                out.push(s.clone());
            }
        }
        Edn::List(v) | Edn::Vector(v) | Edn::Set(v) => v.iter().for_each(|x| collect_vars(x, out)),
        _ => {}
    }
}

fn clause_items(e: &Edn) -> Option<&[Edn]> {
    match e {
        Edn::Vector(v) | Edn::List(v) => Some(v),
        _ => None,
    }
}

fn head_sym(items: &[Edn]) -> Option<&str> {
    match items.first() {
        Some(Edn::Sym(s)) => Some(s),
        _ => None,
    }
}

/// Is this a list-like logic clause (`not`, `or`, ...) or a rule call (as opposed to a pattern
/// or a function clause)?
fn list_clause(e: &Edn) -> Option<(&str, &[Edn])> {
    match e {
        Edn::List(v) => head_sym(v).map(|h| (h, &v[1..])),
        _ => None,
    }
}

fn fn_clause(e: &Edn) -> Option<(&[Edn], Option<&Edn>)> {
    let Edn::Vector(v) = e else { return None };
    match v.first() {
        Some(Edn::List(call)) if !call.is_empty() && v.len() <= 2 => Some((call, v.get(1))),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum Operand {
    V(Val),
    C(Edn),
    E(S),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    fn sql(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "<>",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }

    fn flip(self) -> Op {
        match self {
            Op::Lt => Op::Gt,
            Op::Le => Op::Ge,
            Op::Gt => Op::Lt,
            Op::Ge => Op::Le,
            o => o,
        }
    }
}

fn cmp_op(name: &str) -> Option<Op> {
    Some(match name {
        "=" | "==" => Op::Eq,
        "not=" | "!=" => Op::Ne,
        "<" => Op::Lt,
        "<=" => Op::Le,
        ">" => Op::Gt,
        ">=" => Op::Ge,
        _ => return None,
    })
}

impl<'a> Comp<'a> {
    pub(super) fn new(ctx: &'a QueryContext) -> Self {
        Self {
            ctx,
            types: HashMap::new(),
            n: 0,
            warnings: Vec::new(),
        }
    }

    fn alias(&mut self, p: &str) -> String {
        self.n += 1;
        format!("{p}{}", self.n)
    }

    // ---- type inference -------------------------------------------------------------

    fn note(&mut self, var: &str, ty: Ty) -> R<()> {
        match self.types.get(var) {
            Some(t) if *t != ty => syntax(format!(
                "variable {var} is used both as {} and as {}",
                t.name(),
                ty.name()
            )),
            _ => {
                self.types.insert(var.to_owned(), ty);
                Ok(())
            }
        }
    }

    fn infer(&mut self, clauses: &[Edn]) -> R<()> {
        for c in clauses {
            if let Some((head, args)) = list_clause(c) {
                match head {
                    "not" | "or" | "and" => self.infer(args)?,
                    "not-join" | "or-join" => self.infer(args.get(1..).unwrap_or_default())?,
                    rule => {
                        if let (Some(ty), Some(v)) =
                            (rule_entity_ty(rule), args.first().and_then(var_name))
                        {
                            self.note(v, ty)?;
                        }
                    }
                }
                continue;
            }
            if let Some(items) = clause_items(c) {
                if fn_clause(c).is_some() {
                    continue;
                }
                if items.len() >= 2 {
                    let Ok(attr) = attr_name(&items[1]) else {
                        continue;
                    };
                    if let Some(v) = var_name(&items[0]) {
                        if PAGE_ONLY.contains(&attr.as_str()) {
                            self.note(v, Ty::Page)?;
                        } else if BLOCK_ONLY.contains(&attr.as_str()) {
                            self.note(v, Ty::Block)?;
                        } else if attr.starts_with("file/") {
                            self.note(v, Ty::File)?;
                        }
                    }
                    if let (Some(ty), Some(v)) = (value_ty(&attr), items.get(2).and_then(var_name))
                    {
                        self.note(v, ty)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Defaults for variables with no evidence: values of `:block/refs` are pages.
    fn infer_defaults(&mut self, clauses: &[Edn]) {
        for c in clauses {
            if let Some((head, args)) = list_clause(c) {
                match head {
                    "not" | "or" | "and" => self.infer_defaults(args),
                    "not-join" | "or-join" => {
                        self.infer_defaults(args.get(1..).unwrap_or_default());
                    }
                    _ => {}
                }
            } else if let Some(items) = clause_items(c) {
                // A variable whose only evidence is `:block/properties` is a block (page
                // properties are reached through `:block/name` or `page-property`).
                if items.len() == 3
                    && matches!(&items[1], Edn::Kw(k) if k == "block/properties" || k == "page/properties")
                    && let Some(v) = var_name(&items[0])
                {
                    self.types.entry(v.to_owned()).or_insert(Ty::Block);
                }
                if items.len() == 3
                    && matches!(&items[1], Edn::Kw(k) if k == "block/refs" || k == "page/refs" || k == "block/ref-pages")
                    && let Some(v) = var_name(&items[2])
                {
                    self.types.entry(v.to_owned()).or_insert(Ty::Page);
                }
            }
        }
    }

    pub(super) fn infer_all(&mut self, clauses: &[Edn]) -> R<()> {
        self.infer(clauses)?;
        self.infer_defaults(clauses);
        Ok(())
    }

    // ---- bindings -------------------------------------------------------------------

    fn ty_of(&self, var: &str) -> R<Ty> {
        self.types.get(var).copied().map_or_else(
            || {
                syntax(format!(
                    "cannot tell whether {var} is a page or a block (add a clause that fixes its type)"
                ))
            },
            Ok,
        )
    }

    fn lookup<'s>(&self, sc: &'s Scope, var: &str) -> R<&'s Binding> {
        sc.vars.get(var).map_or_else(
            || {
                unsupported(format!(
                    "variable {var} is not bound here (variables bound only inside or/not \
                     cannot be used outside)"
                ))
            },
            Ok,
        )
    }

    /// Row alias of an entity variable, joining its table when first needed.
    fn row(&mut self, sc: &mut Scope, var: &str) -> R<String> {
        let ty = self.ty_of(var)?;
        match sc.vars.get(var).cloned() {
            Some(Binding::Entity { alias: Some(a), .. }) => Ok(a),
            Some(Binding::Entity { id, ty: bty, .. }) => {
                if bty != ty {
                    return syntax(format!("variable {var} changes type"));
                }
                let a = self.alias(&ty.table()[..1]);
                sc.from.push(format!("{} {a}", ty.table()));
                sc.wh.push(S::new(format!("{a}.id = ")).with(&id));
                sc.vars.insert(
                    var.to_owned(),
                    Binding::Entity {
                        ty,
                        id,
                        alias: Some(a.clone()),
                    },
                );
                Ok(a)
            }
            None => {
                let a = self.alias(&ty.table()[..1]);
                sc.from.push(format!("{} {a}", ty.table()));
                sc.vars.insert(
                    var.to_owned(),
                    Binding::Entity {
                        ty,
                        id: S::new(format!("{a}.id")),
                        alias: Some(a.clone()),
                    },
                );
                Ok(a)
            }
            Some(_) => syntax(format!("variable {var} is not an entity")),
        }
    }

    fn bind_entity(&mut self, sc: &mut Scope, var: &str, ty: Ty, id: S) -> R<()> {
        match self.types.get(var) {
            Some(t) if *t != ty => {
                return syntax(format!(
                    "variable {var} is {} but {} is needed",
                    t.name(),
                    ty.name()
                ));
            }
            Some(_) => {}
            None => {
                self.types.insert(var.to_owned(), ty);
            }
        }
        match sc.vars.get(var) {
            None => {
                sc.vars.insert(
                    var.to_owned(),
                    Binding::Entity {
                        ty,
                        id,
                        alias: None,
                    },
                );
                Ok(())
            }
            Some(Binding::Entity { id: old, .. }) => {
                let w = S::new("").with(old).then(" = ").with(&id);
                sc.wh.push(w);
                Ok(())
            }
            Some(_) => syntax(format!("variable {var} is not an entity")),
        }
    }

    fn bind_val(&mut self, sc: &mut Scope, var: &str, val: Val) -> R<()> {
        match sc.vars.get(var).cloned() {
            None => {
                sc.vars.insert(var.to_owned(), Binding::Val(val));
                Ok(())
            }
            Some(Binding::Val(old)) => {
                let w = self.compare(Op::Eq, &Operand::V(old), &Operand::V(val))?;
                sc.wh.push(w);
                Ok(())
            }
            Some(Binding::Const(c)) => {
                let w = self.cmp_const(&val, Op::Eq, &c)?;
                sc.wh.push(w);
                Ok(())
            }
            Some(_) => syntax(format!(
                "variable {var} is used as an entity and as a value"
            )),
        }
    }

    // ---- constants and comparison ---------------------------------------------------

    fn cmp_const(&self, v: &Val, op: Op, c: &Edn) -> R<S> {
        match c {
            Edn::Str(s) => {
                let s = if v.ci { lower_nfc(s) } else { s.clone() };
                Ok(S::new("")
                    .with(&v.text)
                    .then(&format!(" {} ", op.sql()))
                    .with(&S::param(Value::Text(s))))
            }
            Edn::Int(_) | Edn::Float(_) => match &v.num {
                Some(n) => Ok(S::new("")
                    .with(n)
                    .then(&format!(" {} ", op.sql()))
                    .with(&S::param(const_value(c)?))),
                None => {
                    let t = match c {
                        Edn::Int(n) => n.to_string(),
                        Edn::Float(x) => x.to_string(),
                        _ => String::new(),
                    };
                    Ok(S::new("")
                        .with(&v.text)
                        .then(&format!(" {} ", op.sql()))
                        .with(&S::param(Value::Text(t))))
                }
            },
            Edn::Bool(b) => match &v.num {
                Some(n) => Ok(S::new("")
                    .with(n)
                    .then(&format!(" {} ", op.sql()))
                    .with(&S::param(Value::Integer(i64::from(*b))))),
                None => Ok(S::new("")
                    .with(&v.text)
                    .then(&format!(" {} ", op.sql()))
                    .with(&S::param(text(&b.to_string())))),
            },
            Edn::Nil => unsupported("comparison with nil"),
            other => unsupported(format!("comparison with {other}")),
        }
    }

    fn compare(&self, op: Op, a: &Operand, b: &Operand) -> R<S> {
        match (a, b) {
            (Operand::V(v), Operand::C(c)) => self.cmp_const(v, op, c),
            (Operand::C(c), Operand::V(v)) => self.cmp_const(v, op.flip(), c),
            (Operand::V(x), Operand::V(y)) => {
                let w = if let (Some(nx), Some(ny)) = (&x.num, &y.num) {
                    S::new("")
                        .with(nx)
                        .then(&format!(" {} ", op.sql()))
                        .with(ny)
                } else {
                    let lower = x.ci || y.ci;
                    let side = |v: &Val| {
                        if lower && !v.ci {
                            S::new("lower(").with(&v.text).then(")")
                        } else {
                            v.text.clone()
                        }
                    };
                    S::new("")
                        .with(&side(x))
                        .then(&format!(" {} ", op.sql()))
                        .with(&side(y))
                };
                Ok(w)
            }
            (Operand::C(x), Operand::C(y)) => Ok(S::new("")
                .with(&S::param(const_value(x)?))
                .then(&format!(" {} ", op.sql()))
                .with(&S::param(const_value(y)?))),
            (Operand::E(x), Operand::E(y)) if matches!(op, Op::Eq | Op::Ne) => {
                Ok(S::new("").with(x).then(&format!(" {} ", op.sql())).with(y))
            }
            _ => unsupported("comparison of an entity with a value"),
        }
    }

    fn operand(&self, sc: &Scope, e: &Edn) -> R<Operand> {
        if let Some(v) = var_name(e) {
            return Ok(match self.lookup(sc, v)? {
                Binding::Val(x) => Operand::V(x.clone()),
                Binding::Const(c) => Operand::C(c.clone()),
                Binding::Entity { id, .. } => Operand::E(id.clone()),
                Binding::Props { .. } => return unsupported("using a properties map as a value"),
            });
        }
        Ok(Operand::C(e.clone()))
    }

    // ---- clauses --------------------------------------------------------------------

    pub(super) fn clauses(&mut self, sc: &mut Scope, clauses: &[Edn]) -> R<()> {
        // Stages: patterns/rules, function bindings, predicates, logic.
        let stage = |e: &Edn| -> u8 {
            if let Some((head, _)) = list_clause(e) {
                return if matches!(head, "not" | "not-join" | "or" | "or-join" | "and") {
                    3
                } else {
                    0
                };
            }
            match fn_clause(e) {
                Some((_, Some(_))) => 1,
                Some((_, None)) => 2,
                None => 0,
            }
        };
        for st in 0..4u8 {
            for c in clauses.iter().filter(|c| stage(c) == st) {
                self.clause(sc, c)?;
            }
        }
        Ok(())
    }

    fn clause(&mut self, sc: &mut Scope, c: &Edn) -> R<()> {
        if let Some((head, args)) = list_clause(c) {
            return match head {
                "not" => self.not_clause(sc, args, None),
                "not-join" => {
                    let Some(Edn::Vector(vars)) = args.first() else {
                        return syntax("not-join needs a variable vector");
                    };
                    self.not_clause(sc, &args[1..], Some(vars))
                }
                "or" => self.or_clause(sc, args, None),
                "or-join" => {
                    let Some(Edn::Vector(vars)) = args.first() else {
                        return syntax("or-join needs a variable vector");
                    };
                    self.or_clause(sc, &args[1..], Some(vars))
                }
                "and" => self.clauses(sc, args),
                rule => self.rule_call(sc, rule, args),
            };
        }
        if let Some((call, out)) = fn_clause(c) {
            return match out {
                Some(o) => self.fn_binding(sc, call, o),
                None => {
                    let w = self.predicate(sc, call)?;
                    sc.wh.push(w);
                    Ok(())
                }
            };
        }
        match c {
            Edn::Vector(items) if items.len() >= 2 && items.len() <= 3 => self.pattern(sc, items),
            other => unsupported(format!("clause {other}")),
        }
    }

    fn scoped_child(&self, sc: &Scope, vars: Option<&Vec<Edn>>) -> Scope {
        let mut ch = sc.child();
        if let Some(vs) = vars {
            let keep: Vec<&str> = vs.iter().filter_map(var_name).collect();
            ch.vars.retain(|k, _| keep.contains(&k.as_str()));
        }
        ch
    }

    fn not_clause(&mut self, sc: &mut Scope, args: &[Edn], vars: Option<&Vec<Edn>>) -> R<()> {
        let mut ch = self.scoped_child(sc, vars);
        self.clauses(&mut ch, args)?;
        let body = ch.exists_body();
        sc.wh.push(S::new("NOT EXISTS (").with(&body).then(")"));
        Ok(())
    }

    fn or_clause(&mut self, sc: &mut Scope, args: &[Edn], vars: Option<&Vec<Edn>>) -> R<()> {
        if args.is_empty() {
            return syntax("`or` needs at least one branch");
        }
        // Entity variables that no clause outside the `or` binds range over their whole table;
        // the branches then filter them.
        let mut seen = Vec::new();
        for a in args {
            collect_vars(a, &mut seen);
        }
        for v in seen {
            if !sc.vars.contains_key(&v) && self.types.contains_key(&v) {
                let _ = self.row(sc, &v)?;
            }
        }
        let mut parts: Vec<S> = Vec::new();
        for branch in args {
            let mut ch = self.scoped_child(sc, vars);
            let items: Vec<Edn> = match list_clause(branch) {
                Some(("and", rest)) => rest.to_vec(),
                _ => vec![branch.clone()],
            };
            self.clauses(&mut ch, &items)?;
            parts.push(S::new("EXISTS (").with(&ch.exists_body()).then(")"));
        }
        let mut w = S::new("(");
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                w = w.then(" OR ");
            }
            w = w.with(p);
        }
        sc.wh.push(w.then(")"));
        Ok(())
    }

    // ---- data patterns --------------------------------------------------------------

    fn pattern(&mut self, sc: &mut Scope, items: &[Edn]) -> R<()> {
        let Some(evar) = var_name(&items[0]).map(str::to_owned) else {
            return unsupported("entity constants in data patterns");
        };
        let attr = attr_name(&items[1])?;
        if !known_attr(&attr) {
            return unsupported(format!("attribute :{attr}"));
        }
        let v = items.get(2);
        let ty = self.ty_of(&evar)?;
        let a = self.row(sc, &evar)?;
        let wildcard = v.is_none_or(is_wildcard);

        if let Some((col, vk, nullable)) = column(ty, &a, &attr) {
            let val = Val::from_col(&col, vk);
            if nullable {
                sc.wh.push(S::new(format!("{col} IS NOT NULL")));
            }
            return match v {
                _ if wildcard => Ok(()),
                Some(e) => match var_name(e) {
                    Some(var) => self.bind_val(sc, var, val),
                    None => {
                        let w = self.cmp_const(&val, Op::Eq, e)?;
                        sc.wh.push(w);
                        Ok(())
                    }
                },
                None => Ok(()),
            };
        }

        let target = v.and_then(var_name).map(str::to_owned);
        if !wildcard && target.is_none() {
            return unsupported(format!("constant value for entity attribute :{attr}"));
        }
        match (ty, attr.as_str()) {
            (Ty::Page, "block/namespace") => {
                let id = S::new(format!("{a}.namespace_parent_id"));
                sc.wh
                    .push(S::new(format!("{a}.namespace_parent_id IS NOT NULL")));
                if let Some(t) = target {
                    self.bind_entity(sc, &t, Ty::Page, id)?;
                }
            }
            (Ty::Page, "block/file") => {
                let id = S::new(format!("{a}.file_id"));
                sc.wh.push(S::new(format!("{a}.file_id IS NOT NULL")));
                if let Some(t) = target {
                    self.bind_entity(sc, &t, Ty::File, id)?;
                }
            }
            (Ty::Page, "block/tags") => {
                let j = self.alias("pt");
                sc.from.push(format!("page_tags {j}"));
                sc.wh.push(S::new(format!("{j}.page_id = {a}.id")));
                if let Some(t) = target {
                    self.bind_entity(sc, &t, Ty::Page, S::new(format!("{j}.tag_page_id")))?;
                }
            }
            (Ty::Page, "block/alias") => {
                let j = self.alias("pa");
                sc.from.push(format!("page_aliases {j}"));
                sc.wh.push(S::new(format!(
                    "({j}.page_id = {a}.id OR {j}.alias_page_id = {a}.id)"
                )));
                if let Some(t) = target {
                    let id = S::new(format!(
                        "(CASE WHEN {j}.page_id = {a}.id THEN {j}.alias_page_id ELSE {j}.page_id END)"
                    ));
                    self.bind_entity(sc, &t, Ty::Page, id)?;
                }
            }
            (Ty::Block, "block/page") => {
                if let Some(t) = target {
                    self.bind_entity(sc, &t, Ty::Page, S::new(format!("{a}.page_id")))?;
                }
            }
            (Ty::Block, "block/parent") => {
                if let Some(t) = target {
                    match self.ty_of(&t)? {
                        Ty::Block => {
                            sc.wh.push(S::new(format!("{a}.parent_id IS NOT NULL")));
                            self.bind_entity(sc, &t, Ty::Block, S::new(format!("{a}.parent_id")))?;
                        }
                        Ty::Page => {
                            sc.wh.push(S::new(format!("{a}.parent_id IS NULL")));
                            self.bind_entity(sc, &t, Ty::Page, S::new(format!("{a}.page_id")))?;
                        }
                        Ty::File => return syntax("a block's parent cannot be a file"),
                    }
                }
            }
            (Ty::Block, "block/refs") => {
                let want = match &target {
                    Some(t) => self.types.get(t).copied().unwrap_or(Ty::Page),
                    None => Ty::Page,
                };
                match want {
                    Ty::Page => {
                        let j = self.alias("r");
                        sc.from.push(format!("block_page_refs {j}"));
                        sc.wh.push(S::new(format!("{j}.block_id = {a}.id")));
                        if let Some(t) = target {
                            self.bind_entity(sc, &t, Ty::Page, S::new(format!("{j}.page_id")))?;
                        }
                    }
                    Ty::Block => {
                        let j = self.alias("br");
                        let tb = self.alias("b");
                        sc.from.push(format!("block_block_refs {j}"));
                        sc.from.push(format!("blocks {tb}"));
                        sc.wh.push(S::new(format!("{j}.block_id = {a}.id")));
                        sc.wh.push(S::new(format!("{tb}.uuid = {j}.target_uuid")));
                        if let Some(t) = target {
                            self.bind_entity(sc, &t, Ty::Block, S::new(format!("{tb}.id")))?;
                        }
                    }
                    Ty::File => return syntax("refs cannot point to a file"),
                }
            }
            (Ty::Block, "block/path-refs") => {
                let j = self.alias("pr");
                sc.from.push(format!("block_path_refs {j}"));
                sc.wh.push(S::new(format!("{j}.block_id = {a}.id")));
                if let Some(t) = target {
                    self.bind_entity(sc, &t, Ty::Page, S::new(format!("{j}.page_id")))?;
                }
            }
            (Ty::Block | Ty::Page, "block/properties") => {
                if let Some(t) = target {
                    match sc.vars.get(&t) {
                        None => {
                            sc.vars.insert(t, Binding::Props { owner: evar });
                        }
                        Some(_) => return unsupported("rebinding a properties map"),
                    }
                }
            }
            (_, other) => {
                return unsupported(format!("attribute :{other} on {}", ty.name()));
            }
        }
        Ok(())
    }

    // ---- DSL rules ------------------------------------------------------------------

    fn rule_call(&mut self, sc: &mut Scope, rule: &str, args: &[Edn]) -> R<()> {
        if !RULES.contains(&rule) {
            return unsupported(format!(
                "rule `{rule}` (only the built-in DSL rules are available)"
            ));
        }
        let Some(var) = args.first().and_then(var_name).map(str::to_owned) else {
            return syntax(format!("rule `{rule}` needs an entity variable"));
        };
        let ty = rule_entity_ty(rule).unwrap_or(Ty::Block);
        self.types.entry(var.clone()).or_insert(ty);
        let _ = self.row(sc, &var)?;
        let id = match sc.vars.get(&var) {
            Some(Binding::Entity { id, .. }) => id.clone(),
            _ => return syntax(format!("{var} is not an entity")),
        };
        let rest = &args[1..];
        let arg = |i: usize| -> R<Edn> {
            let e = rest.get(i).ok_or_else(|| {
                QueryError::Syntax(format!("rule `{rule}` is missing an argument"))
            })?;
            // A variable bound to a constant stands for that constant.
            if let Some(v) = var_name(e) {
                return match self.lookup(sc, v)? {
                    Binding::Const(c) => Ok(c.clone()),
                    _ => syntax(format!("rule argument {v} must be a constant")),
                };
            }
            Ok(e.clone())
        };
        let key_of = |e: &Edn| -> R<String> {
            match e {
                Edn::Kw(k) | Edn::Str(k) | Edn::Sym(k) => Ok(norm_key(k)),
                other => syntax(format!("expected a property key, got {other}")),
            }
        };
        let leaf: Query = match rule {
            "page-ref" => Query::PageRef(page_key_of(&arg(0)?)?),
            "task" => Query::Task(
                str_set(&arg(0)?)?
                    .iter()
                    .map(|s| s.to_uppercase())
                    .collect(),
            ),
            "priority" => Query::Priority(
                str_set(&arg(0)?)?
                    .iter()
                    .map(|s| s.to_uppercase())
                    .collect(),
            ),
            "property" | "page-property" => {
                let key = key_of(&arg(0)?)?;
                let value = match rest.get(1) {
                    Some(_) => Some(parse_prop_value(&arg(1)?)?),
                    None => None,
                };
                if rule == "property" {
                    Query::Property { key, value }
                } else {
                    Query::PageProperty { key, value }
                }
            }
            "has-property" => Query::Property {
                key: key_of(&arg(0)?)?,
                value: None,
            },
            "has-page-property" => Query::PageProperty {
                key: key_of(&arg(0)?)?,
                value: None,
            },
            "page" => Query::Page(page_key_of(&arg(0)?)?),
            "namespace" => Query::Namespace(page_key_of(&arg(0)?)?),
            "page-tags" => Query::PageTags(
                str_set(&arg(0)?)?
                    .iter()
                    .map(|s| page_key_of(&Edn::Str(s.clone())))
                    .collect::<R<Vec<_>>>()?,
            ),
            "all-page-tags" => Query::AllPageTags,
            "between" => {
                let (a, b) = (arg(0)?, arg(1)?);
                let (Edn::Int(a), Edn::Int(b)) = (a, b) else {
                    return syntax("`between` rule takes two journal-day integers");
                };
                let (lo, hi) = (a.min(b), a.max(b));
                let w = S::new("EXISTS (SELECT 1 FROM blocks b WHERE b.id = ")
                    .with(&id)
                    .then(
                        " AND b.page_id IN (SELECT id FROM pages WHERE is_journal = 1 \
                     AND journal_day BETWEEN ? AND ?))",
                    );
                let mut w = w;
                w.params.push(Value::Integer(lo));
                w.params.push(Value::Integer(hi));
                sc.wh.push(w);
                return Ok(());
            }
            "block-content" => {
                let Edn::Str(needle) = arg(0)? else {
                    return syntax("`block-content` takes a string");
                };
                let w = S::new("EXISTS (SELECT 1 FROM blocks b WHERE b.id = ")
                    .with(&id)
                    .then(" AND instr(b.content, ")
                    .with(&S::param(Value::Text(needle)))
                    .then(") > 0)");
                sc.wh.push(w);
                return Ok(());
            }
            _ => return unsupported(format!("rule `{rule}`")),
        };
        let blocks = ty == Ty::Block;
        let (sql, params) = super::compile::leaf_sql(&leaf, self.ctx, blocks)?;
        let alias = if blocks { "b" } else { "p" };
        let table = ty.table();
        let mut w = S::new(format!(
            "EXISTS (SELECT 1 FROM {table} {alias} WHERE {alias}.id = "
        ))
        .with(&id)
        .then(" AND ");
        w.sql.push_str(&sql);
        w.params.extend(params);
        sc.wh.push(w.then(")"));
        Ok(())
    }

    // ---- predicates and functions ---------------------------------------------------

    fn predicate(&mut self, sc: &mut Scope, call: &[Edn]) -> R<S> {
        let Some(name) = head_sym(call) else {
            return syntax("a predicate must start with a function name");
        };
        let args = &call[1..];
        let name = name.rsplit('/').next().unwrap_or(name);
        let full = head_sym(call).unwrap_or(name);
        if let Some(op) = cmp_op(name).filter(|_| !full.contains('/')) {
            if args.len() != 2 {
                return unsupported(format!("`{name}` with {} arguments", args.len()));
            }
            let (a, b) = (self.operand(sc, &args[0])?, self.operand(sc, &args[1])?);
            return self.compare(op, &a, &b);
        }
        match name {
            "contains?" => {
                if args.len() != 2 {
                    return syntax("contains? takes a collection and an item");
                }
                let coll = self.operand(sc, &args[0])?;
                let item = self.operand(sc, &args[1])?;
                let (Operand::C(Edn::Set(items) | Edn::Vector(items)), Operand::V(v)) =
                    (&coll, &item)
                else {
                    return unsupported("contains? other than (contains? #{...} ?value)");
                };
                if items.is_empty() {
                    return Ok(S::new("0"));
                }
                let mut s = S::new("(");
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        s = s.then(" OR ");
                    }
                    s = s.with(&self.cmp_const(v, Op::Eq, it)?);
                }
                Ok(s.then(")"))
            }
            "includes?" | "starts-with?" | "ends-with?" => {
                if args.len() != 2 {
                    return syntax(format!("{name} takes a string and a fragment"));
                }
                let hay = self.operand(sc, &args[0])?;
                let needle = self.operand(sc, &args[1])?;
                let Operand::V(h) = hay else {
                    return unsupported(format!("{name} on a constant string"));
                };
                let n = match needle {
                    Operand::C(Edn::Str(s)) => {
                        S::param(Value::Text(if h.ci { lower_nfc(&s) } else { s }))
                    }
                    Operand::V(v) => v.text,
                    _ => return unsupported(format!("{name} with this fragment")),
                };
                Ok(match name {
                    "includes?" => S::new("instr(")
                        .with(&h.text)
                        .then(", ")
                        .with(&n)
                        .then(") > 0"),
                    "starts-with?" => S::new("instr(")
                        .with(&h.text)
                        .then(", ")
                        .with(&n)
                        .then(") = 1"),
                    _ => S::new("(length(")
                        .with(&h.text)
                        .then(") >= length(")
                        .with(&n)
                        .then(") AND substr(")
                        .with(&h.text)
                        .then(", length(")
                        .with(&h.text)
                        .then(") - length(")
                        .with(&n)
                        .then(") + 1) = ")
                        .with(&n)
                        .then(")"),
                })
            }
            "re-find" | "re-matches" => {
                if args.len() != 2 {
                    return syntax(format!("{name} takes a pattern and a string"));
                }
                let pat = self.operand(sc, &args[0])?;
                let hay = self.operand(sc, &args[1])?;
                let p = match pat {
                    Operand::C(Edn::Regex(s) | Edn::Str(s)) => s,
                    _ => return unsupported("a regular expression that is not a constant"),
                };
                let p = if name == "re-matches" {
                    format!("^(?:{p})$")
                } else {
                    p
                };
                regex::Regex::new(&p)
                    .map_err(|e| QueryError::Syntax(format!("invalid regular expression: {e}")))?;
                let Operand::V(h) = hay else {
                    return unsupported("a regular expression over a constant");
                };
                Ok(S::new("bitacora_regexp(")
                    .with(&S::param(Value::Text(p)))
                    .then(", ")
                    .with(&h.text)
                    .then(")"))
            }
            "missing?" | "some?" => {
                let items: Vec<&Edn> = args
                    .iter()
                    .filter(|e| !matches!(e, Edn::Sym(s) if s == "$"))
                    .collect();
                let [e, attr] = items.as_slice() else {
                    return unsupported(format!("{name} other than ({name} $ ?e :attr)"));
                };
                let Some(var) = var_name(e) else {
                    return unsupported(format!("{name} on a constant"));
                };
                let has = self.has_attr(sc, var, &attr_name(attr)?)?;
                Ok(if name == "missing?" {
                    S::new("NOT (").with(&has).then(")")
                } else {
                    has
                })
            }
            other => unsupported(format!("function `{other}`")),
        }
    }

    /// Whether the entity has a value for `attr`, as a boolean SQL expression.
    fn has_attr(&mut self, sc: &mut Scope, var: &str, attr: &str) -> R<S> {
        let ty = self.ty_of(var)?;
        let a = self.row(sc, var)?;
        if let Some((col, _, nullable)) = column(ty, &a, attr) {
            return Ok(S::new(if nullable {
                format!("{col} IS NOT NULL")
            } else {
                "1".to_owned()
            }));
        }
        let known =
            PAGE_ONLY.contains(&attr) || BLOCK_ONLY.contains(&attr) || attr == "block/properties";
        if !known {
            return unsupported(format!("attribute :{attr}"));
        }
        Ok(S::new(match (ty, attr) {
            (Ty::Page, "block/namespace") => format!("{a}.namespace_parent_id IS NOT NULL"),
            (Ty::Page, "block/file") => format!("{a}.file_id IS NOT NULL"),
            (Ty::Page, "block/tags") => {
                format!("EXISTS (SELECT 1 FROM page_tags t WHERE t.page_id = {a}.id)")
            }
            (Ty::Page, "block/alias") => format!(
                "EXISTS (SELECT 1 FROM page_aliases t WHERE t.page_id = {a}.id OR t.alias_page_id = {a}.id)"
            ),
            (Ty::Block, "block/page") => "1".to_owned(),
            (Ty::Block, "block/parent") => "1".to_owned(),
            (Ty::Block, "block/refs") => {
                format!("EXISTS (SELECT 1 FROM block_page_refs t WHERE t.block_id = {a}.id)")
            }
            (Ty::Block, "block/path-refs") => "1".to_owned(),
            (Ty::Block, "block/properties") => {
                format!("EXISTS (SELECT 1 FROM block_properties t WHERE t.block_id = {a}.id)")
            }
            // The attribute does not exist for this entity type (`:block/name` of a block).
            _ => "0".to_owned(),
        }))
    }

    fn fn_binding(&mut self, sc: &mut Scope, call: &[Edn], out: &Edn) -> R<()> {
        let Some(outvar) = var_name(out).map(str::to_owned) else {
            return unsupported("function result bindings other than a single variable");
        };
        let Some(full) = head_sym(call) else {
            return syntax("a function call must start with a function name");
        };
        let name = full.rsplit('/').next().unwrap_or(full);
        let args = &call[1..];
        match name {
            "get" => {
                let (Some(pv), Some(Edn::Kw(key))) = (args.first().and_then(var_name), args.get(1))
                else {
                    return unsupported("get other than (get ?props :key)");
                };
                if args.len() != 2 {
                    return unsupported("get with a default value");
                }
                let Binding::Props { owner } = self.lookup(sc, pv)?.clone() else {
                    return unsupported("get on something that is not a :block/properties map");
                };
                let ty = self.ty_of(&owner)?;
                let a = self.row(sc, &owner)?;
                let j = self.alias("pv");
                let (tbl, col) = if ty == Ty::Block {
                    ("block_property_values", "block_id")
                } else {
                    ("page_property_values", "page_id")
                };
                sc.from.push(format!("{tbl} {j}"));
                sc.wh.push(
                    S::new(format!("{j}.{col} = {a}.id AND {j}.key = "))
                        .with(&S::param(Value::Text(norm_key(key)))),
                );
                let val = Val {
                    text: S::new(format!("{j}.value_norm")),
                    num: Some(S::new(format!("{j}.value_num"))),
                    ci: true,
                };
                self.bind_val(sc, &outvar, val)
            }
            "get-else" => {
                let items: Vec<&Edn> = args
                    .iter()
                    .filter(|e| !matches!(e, Edn::Sym(s) if s == "$"))
                    .collect();
                let [e, attr, default] = items.as_slice() else {
                    return unsupported("get-else other than (get-else $ ?e :attr default)");
                };
                let Some(var) = var_name(e) else {
                    return unsupported("get-else on a constant");
                };
                let attr = attr_name(attr)?;
                let ty = self.ty_of(var)?;
                let a = self.row(sc, var)?;
                let Some((col, vk, _)) = column(ty, &a, &attr) else {
                    return unsupported(format!("get-else on attribute :{attr}"));
                };
                let d = S::param(const_value(default)?);
                let base = Val::from_col(&col, vk);
                let coalesced = match vk {
                    Vk::Text => Val {
                        text: S::new("COALESCE(").then(&col).then(", ").with(&d).then(")"),
                        num: None,
                        ci: false,
                    },
                    _ => Val {
                        text: base.text,
                        num: Some(S::new("COALESCE(").then(&col).then(", ").with(&d).then(")")),
                        ci: false,
                    },
                };
                self.bind_val(sc, &outvar, coalesced)
            }
            "str" => {
                let mut s = S::new("(");
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        s = s.then(" || ");
                    }
                    match self.operand(sc, a)? {
                        Operand::V(v) => s = s.with(&v.text),
                        Operand::C(Edn::Str(x)) => s = s.with(&S::param(Value::Text(x))),
                        Operand::C(Edn::Int(n)) => s = s.with(&S::param(text(&n.to_string()))),
                        _ => return unsupported("str of this argument"),
                    }
                }
                if args.is_empty() {
                    s = s.then("''");
                }
                let val = Val {
                    text: s.then(")"),
                    num: None,
                    ci: false,
                };
                self.bind_val(sc, &outvar, val)
            }
            "re-pattern" | "identity" | "ground" => {
                let [a] = args else {
                    return syntax(format!("{name} takes one argument"));
                };
                let c = match (name, self.operand(sc, a)?) {
                    ("re-pattern", Operand::C(Edn::Str(s) | Edn::Regex(s))) => Edn::Regex(s),
                    (_, Operand::C(c)) => c,
                    _ => return unsupported(format!("{name} of a non-constant")),
                };
                self.bind_const(sc, &outvar, c)
            }
            "lower-case" | "upper-case" => {
                let [a] = args else {
                    return syntax(format!("{name} takes one argument"));
                };
                let Operand::V(v) = self.operand(sc, a)? else {
                    return unsupported(format!("{name} of a constant"));
                };
                let f = if name == "lower-case" {
                    "lower"
                } else {
                    "upper"
                };
                let val = Val {
                    text: S::new(format!("{f}(")).with(&v.text).then(")"),
                    num: None,
                    ci: false,
                };
                self.bind_val(sc, &outvar, val)
            }
            "+" | "-" | "*" => {
                let [x, y] = args else {
                    return unsupported(format!("`{name}` with {} arguments", args.len()));
                };
                let num = |o: Operand| -> R<S> {
                    match o {
                        Operand::V(Val { num: Some(n), .. }) => Ok(n),
                        Operand::C(c @ (Edn::Int(_) | Edn::Float(_))) => {
                            Ok(S::param(const_value(&c)?))
                        }
                        _ => unsupported(format!("`{name}` over non-numeric values")),
                    }
                };
                let (x, y) = (num(self.operand(sc, x)?)?, num(self.operand(sc, y)?)?);
                let e = S::new("(")
                    .with(&x)
                    .then(&format!(" {name} "))
                    .with(&y)
                    .then(")");
                let val = Val {
                    text: S::new("CAST(").with(&e).then(" AS TEXT)"),
                    num: Some(e),
                    ci: false,
                };
                self.bind_val(sc, &outvar, val)
            }
            other => unsupported(format!("function `{other}`")),
        }
    }

    fn bind_const(&mut self, sc: &mut Scope, var: &str, c: Edn) -> R<()> {
        match sc.vars.get(var) {
            None => {
                sc.vars.insert(var.to_owned(), Binding::Const(c));
                Ok(())
            }
            Some(_) => unsupported("rebinding a variable with a function result"),
        }
    }

    // ---- inputs ---------------------------------------------------------------------

    pub(super) fn bind_input(&mut self, sc: &mut Scope, var: &str, input: &Edn) -> R<()> {
        let b = match input {
            Edn::Kw(k) => self.keyword_input(k)?,
            Edn::Str(s) => {
                let t = s.trim();
                match t.strip_prefix("[[").and_then(|r| r.strip_suffix("]]")) {
                    Some(inner) => {
                        Binding::Const(Edn::Str(bitacora_core::naming::page_key(inner.trim())))
                    }
                    None => Binding::Const(Edn::Str(s.clone())),
                }
            }
            c @ (Edn::Int(_) | Edn::Float(_) | Edn::Bool(_)) => Binding::Const(c.clone()),
            other => return unsupported(format!("input {other}")),
        };
        sc.vars.insert(var.to_owned(), b);
        Ok(())
    }

    fn keyword_input(&mut self, k: &str) -> R<Binding> {
        let ctx = self.ctx;
        let lc = k.to_lowercase();
        let page = || {
            ctx.current_page
                .clone()
                .map(|p| Binding::Const(Edn::Str(p)))
                .ok_or_else(|| {
                    QueryError::Syntax(format!("input :{k} needs the page the query is on"))
                })
        };
        let day = |arg: &str| -> R<Binding> {
            Ok(Binding::Const(Edn::Int(dates::journal_day(arg, ctx)?)))
        };
        Ok(match lc.as_str() {
            "current-page" | "query-page" => page()?,
            "current-block" => {
                let uuid = ctx.current_block.clone().ok_or_else(|| {
                    QueryError::Syntax("input :current-block needs the current block".into())
                })?;
                Binding::Entity {
                    ty: Ty::Block,
                    id: S::new("(SELECT id FROM blocks WHERE uuid = ")
                        .with(&S::param(Value::Text(uuid)))
                        .then(")"),
                    alias: None,
                }
            }
            "parent-block" => {
                let uuid = ctx.current_block.clone().ok_or_else(|| {
                    QueryError::Syntax("input :parent-block needs the current block".into())
                })?;
                Binding::Entity {
                    ty: Ty::Block,
                    id: S::new("(SELECT parent_id FROM blocks WHERE uuid = ")
                        .with(&S::param(Value::Text(uuid)))
                        .then(")"),
                    alias: None,
                }
            }
            "today" | "yesterday" | "tomorrow" => day(&lc)?,
            "right-now-ms" => Binding::Const(Edn::Int(ctx.now_ms)),
            "start-of-today-ms" => Binding::Const(Edn::Int(ctx.today_start_ms)),
            "end-of-today-ms" => Binding::Const(Edn::Int(ctx.today_start_ms + 86_400_000 - 1)),
            _ => {
                if let Some(base) = lc.strip_suffix("-start") {
                    Binding::Const(Edn::Int(dates::timestamp(base, ctx)?))
                } else if let Some(base) = lc.strip_suffix("-end") {
                    Binding::Const(Edn::Int(dates::timestamp(base, ctx)? + 86_400_000 - 1))
                } else if let Some(base) = lc.strip_suffix("d-before-ms") {
                    Binding::Const(Edn::Int(dates::timestamp(&format!("-{base}d"), ctx)?))
                } else if let Some(base) = lc.strip_suffix("d-before") {
                    day(&format!("-{base}d"))?
                } else if lc.len() > 1
                    && lc
                        .trim_start_matches(['+', '-'])
                        .starts_with(|c: char| c.is_ascii_digit())
                    && lc.ends_with(['d', 'w', 'm', 'y'])
                {
                    day(&lc)?
                } else {
                    return unsupported(format!("input keyword :{k}"));
                }
            }
        })
    }

    // ---- find -----------------------------------------------------------------------

    pub(super) fn top_scope(&self) -> Scope {
        Scope::default()
    }

    /// Output expression and kind of a find variable.
    fn find_col(&mut self, sc: &mut Scope, var: &str) -> R<(S, ColKind, Option<Val>)> {
        match self.lookup(sc, var)?.clone() {
            Binding::Entity { ty, .. } => {
                let _ = self.row(sc, var)?;
                let Some(Binding::Entity { id, .. }) = sc.vars.get(var) else {
                    return syntax("entity lost");
                };
                let kind = match ty {
                    Ty::Block => ColKind::Block,
                    Ty::Page => ColKind::Page,
                    Ty::File => ColKind::File,
                };
                if ty == Ty::File {
                    return Ok((
                        S::new("(SELECT path FROM files WHERE id = ")
                            .with(id)
                            .then(")"),
                        kind,
                        None,
                    ));
                }
                Ok((id.clone(), kind, None))
            }
            Binding::Val(v) => Ok((v.output(), ColKind::Value, Some(v))),
            Binding::Const(c) => Ok((S::param(const_value(&c)?), ColKind::Value, None)),
            Binding::Props { .. } => unsupported("returning a properties map"),
        }
    }

    pub(super) fn find(&mut self, sc: &mut Scope, find: &[Edn], with: &[Edn]) -> R<FindSpec> {
        let _ = with;
        let (elems, shape): (Vec<&Edn>, Shape) = match find {
            [Edn::Vector(v)] if v.len() == 2 && matches!(&v[1], Edn::Sym(s) if s == "...") => {
                (vec![&v[0]], Shape::Collection)
            }
            [Edn::Vector(v)] => (v.iter().collect(), Shape::Tuple),
            [x, Edn::Sym(dot)] if dot == "." => (vec![x], Shape::Scalar),
            other => (other.iter().collect(), Shape::Relation),
        };
        if elems.is_empty() {
            return syntax("empty :find");
        }
        let mut exprs = Vec::new();
        let mut kinds = Vec::new();
        let mut names = Vec::new();
        let mut aggs: Vec<Option<&'static str>> = Vec::new();
        for e in elems {
            names.push(e.to_string());
            match e {
                Edn::Sym(s) if s.starts_with('?') => {
                    let (x, k, _) = self.find_col(sc, s)?;
                    exprs.push(x);
                    kinds.push(k);
                    aggs.push(None);
                }
                Edn::List(items) => {
                    let Some(head) = head_sym(items) else {
                        return syntax("bad :find element");
                    };
                    match head {
                        "pull" => {
                            let Some(v) = items.get(1).and_then(var_name) else {
                                return syntax("pull needs an entity variable");
                            };
                            if let Some(p) = items.get(2) {
                                self.check_pull(p);
                            }
                            let (x, k, _) = self.find_col(sc, v)?;
                            if k == ColKind::Value {
                                return syntax("pull needs an entity variable");
                            }
                            exprs.push(x);
                            kinds.push(k);
                            aggs.push(None);
                        }
                        "count" | "count-distinct" | "sum" | "min" | "max" | "avg" => {
                            let [_, a] = items.as_slice() else {
                                return unsupported(format!("aggregate form {e}"));
                            };
                            let Some(v) = var_name(a) else {
                                return unsupported(format!("aggregate form {e}"));
                            };
                            let (x, _, val) = self.find_col(sc, v)?;
                            // Numbers aggregate over their numeric view.
                            let x = match (&val, head) {
                                (Some(Val { num: Some(n), .. }), "sum" | "min" | "max" | "avg") => {
                                    n.clone()
                                }
                                _ => x,
                            };
                            exprs.push(x);
                            kinds.push(ColKind::Value);
                            aggs.push(Some(match head {
                                "count" => "COUNT",
                                "count-distinct" => "COUNT_DISTINCT",
                                "sum" => "SUM",
                                "min" => "MIN",
                                "max" => "MAX",
                                _ => "AVG",
                            }));
                        }
                        other => return unsupported(format!("find function `{other}`")),
                    }
                }
                other => return unsupported(format!("find element {other}")),
            }
        }
        Ok((exprs, kinds, names, aggs, shape))
    }

    /// Pull patterns other than `[*]` or plain attribute lists are noted as unsupported; the
    /// whole entity is returned regardless.
    fn check_pull(&mut self, pat: &Edn) {
        let fine = match pat {
            Edn::Vector(v) => v.iter().all(|e| match e {
                Edn::Sym(s) => s == "*",
                Edn::Kw(k) => !k.contains("/_"),
                _ => false,
            }),
            _ => false,
        };
        if !fine {
            self.warnings
                .push(format!("pull pattern {pat} (the full entity is returned)"));
        }
    }

    pub(super) fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// Assembles the final SQL of a conjunctive query.
pub(super) fn assemble(
    sc: &Scope,
    exprs: &[S],
    aggs: &[Option<&'static str>],
    limit_one: bool,
) -> S {
    let cols_sql = |with_alias: bool| -> S {
        let mut s = S::new("");
        for (i, e) in exprs.iter().enumerate() {
            if i > 0 {
                s = s.then(", ");
            }
            s = s.with(e);
            if with_alias {
                s = s.then(&format!(" AS c{i}"));
            }
        }
        s
    };
    let mut inner = S::new("SELECT DISTINCT ").with(&cols_sql(true));
    if !sc.from.is_empty() {
        inner = inner.then(" FROM ").then(&sc.from.join(", "));
    }
    if !sc.wh.is_empty() {
        inner = inner.then(" WHERE ");
        for (i, w) in sc.wh.iter().enumerate() {
            if i > 0 {
                inner = inner.then(" AND ");
            }
            inner = inner.with(w);
        }
    }
    let mut out = if aggs.iter().any(Option::is_some) {
        let mut outer = S::new("SELECT ");
        let mut group = Vec::new();
        for (i, a) in aggs.iter().enumerate() {
            if i > 0 {
                outer = outer.then(", ");
            }
            match a {
                None => {
                    outer = outer.then(&format!("c{i}"));
                    group.push(format!("c{i}"));
                }
                Some("COUNT_DISTINCT") => outer = outer.then(&format!("COUNT(DISTINCT c{i})")),
                Some(f) => outer = outer.then(&format!("{f}(c{i})")),
            }
        }
        outer = outer.then(" FROM (").with(&inner).then(")");
        if !group.is_empty() {
            outer = outer.then(" GROUP BY ").then(&group.join(", "));
        }
        outer
    } else {
        let order: Vec<String> = (1..=exprs.len()).map(|i| i.to_string()).collect();
        inner.then(" ORDER BY ").then(&order.join(", "))
    };
    if limit_one {
        out = out.then(" LIMIT 1");
    }
    out
}

impl S {
    pub(super) fn into_parts(self) -> (String, Vec<Value>) {
        (self.sql, self.params)
    }
}
