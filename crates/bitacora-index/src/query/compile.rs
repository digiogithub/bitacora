//! Simple DSL to parameterised SQL (design §7.2). Every user-supplied value is bound as a
//! parameter; the SQL text only contains fixed fragments.

use rusqlite::types::Value;

use super::dates;
use super::dsl::{PropValue, Query, SimpleQuery, SortBy, lower_nfc};
use super::{Compiled, QueryContext, QueryError, ResultKind};
use crate::normalize::normalize_query;
use crate::read::{BLOCK_COLS, PAGE_COLS};
use crate::search::quote;

struct Gen<'a> {
    ctx: &'a QueryContext,
    params: Vec<Value>,
    blocks: bool,
}

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

const PAGE_ID: &str = "(SELECT id FROM pages WHERE name = ?)";

impl Gen<'_> {
    fn p(&mut self, v: Value) {
        self.params.push(v);
    }

    fn placeholders(&mut self, items: &[String]) -> String {
        for s in items {
            self.p(text(s));
        }
        vec!["?"; items.len()].join(",")
    }

    fn expr(&mut self, q: &Query) -> Result<String, QueryError> {
        match q {
            Query::And(v) => self.join(v, " AND "),
            Query::Or(v) => self.join(v, " OR "),
            Query::Not(v) => {
                let inner = self.join(v, " AND ")?;
                Ok(format!("NOT {inner}"))
            }
            _ if self.blocks => self.block_leaf(q),
            _ => self.page_leaf(q),
        }
    }

    fn join(&mut self, v: &[Query], op: &str) -> Result<String, QueryError> {
        let parts = v
            .iter()
            .map(|q| self.expr(q))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("({})", parts.join(op)))
    }

    fn prop_match(&mut self, value: &Option<PropValue>) -> String {
        match value {
            None => String::new(),
            Some(PropValue::Int(n)) => {
                self.p(text(&n.to_string()));
                #[allow(clippy::cast_precision_loss)]
                self.p(Value::Real(*n as f64));
                " AND (value_norm = ? OR value_num = ?)".to_owned()
            }
            Some(PropValue::Bool(b)) => {
                self.p(text(&b.to_string()));
                " AND value_norm = ?".to_owned()
            }
            Some(PropValue::Str(s)) => {
                self.p(text(&lower_nfc(s)));
                " AND value_norm = ?".to_owned()
            }
        }
    }

    fn block_leaf(&mut self, q: &Query) -> Result<String, QueryError> {
        Ok(match q {
            Query::PageRef(name) => {
                self.p(text(name));
                self.p(text(name));
                format!(
                    "b.id IN (SELECT d.id FROM block_page_refs r \
                     JOIN blocks a ON a.id = r.block_id \
                     JOIN blocks d ON d.file_id = a.file_id AND d.ord BETWEEN a.ord AND a.subtree_end \
                     WHERE r.page_id = {PAGE_ID} \
                     UNION SELECT id FROM blocks WHERE page_id = {PAGE_ID})"
                )
            }
            Query::Text(s) => {
                let norm = normalize_query(s, self.ctx.remove_accents);
                if norm.chars().count() >= 3 && self.ctx.trigram {
                    self.p(text(&quote(&norm)));
                    "b.id IN (SELECT rowid FROM blocks_fts_tri WHERE blocks_fts_tri MATCH ?)"
                        .to_owned()
                } else {
                    self.p(text(&norm));
                    "instr(b.search_text, ?) > 0".to_owned()
                }
            }
            Query::Task(m) => {
                let ph = self.placeholders(m);
                format!("COALESCE(b.marker IN ({ph}), 0)")
            }
            Query::Priority(m) => {
                let ph = self.placeholders(m);
                format!("COALESCE(b.priority IN ({ph}), 0)")
            }
            Query::Property { key, value } => {
                self.p(text(key));
                let m = self.prop_match(value);
                format!("b.id IN (SELECT block_id FROM block_property_values WHERE key = ?{m})")
            }
            Query::Between(a, c) => {
                let (mut lo, mut hi) = (
                    dates::journal_day(a, self.ctx)?,
                    dates::journal_day(c, self.ctx)?,
                );
                if lo > hi {
                    std::mem::swap(&mut lo, &mut hi);
                }
                self.p(Value::Integer(lo));
                self.p(Value::Integer(hi));
                "b.page_id IN (SELECT id FROM pages WHERE is_journal = 1 \
                 AND journal_day BETWEEN ? AND ?)"
                    .to_owned()
            }
            Query::BetweenProp { key, from, to } => {
                let (mut lo, mut hi) = (
                    dates::timestamp(from, self.ctx)?,
                    dates::timestamp(to, self.ctx)?,
                );
                if lo > hi {
                    std::mem::swap(&mut lo, &mut hi);
                }
                let col = if key == "created-at" {
                    "created_at"
                } else {
                    "updated_at"
                };
                self.p(text(key));
                #[allow(clippy::cast_precision_loss)]
                {
                    self.p(Value::Real(lo as f64));
                    self.p(Value::Real(hi as f64));
                }
                self.p(Value::Integer(lo));
                self.p(Value::Integer(hi));
                format!(
                    "(b.id IN (SELECT block_id FROM block_property_values \
                     WHERE key = ? AND value_num >= ? AND value_num < ?) \
                     OR COALESCE(b.{col} >= ? AND b.{col} < ?, 0))"
                )
            }
            Query::Page(name) => {
                self.p(text(name));
                format!("COALESCE(b.page_id = {PAGE_ID}, 0)")
            }
            // Page-level leaves bind through the block's page (`[?b :block/page ?p]`).
            other => {
                let inner = self.page_leaf(other)?;
                format!("b.page_id IN (SELECT p.id FROM pages p WHERE {inner})")
            }
        })
    }

    fn page_leaf(&mut self, q: &Query) -> Result<String, QueryError> {
        Ok(match q {
            Query::PageProperty { key, value } => {
                self.p(text(key));
                let m = self.prop_match(value);
                if value.is_some() {
                    format!("p.id IN (SELECT page_id FROM page_property_values WHERE key = ?{m})")
                } else {
                    "p.id IN (SELECT page_id FROM page_properties WHERE key = ?)".to_owned()
                }
            }
            Query::Namespace(name) => {
                self.p(text(name));
                format!("COALESCE(p.namespace_parent_id = {PAGE_ID}, 0)")
            }
            Query::PageTags(tags) => {
                let ph = self.placeholders(tags);
                format!(
                    "p.id IN (SELECT page_id FROM page_tags WHERE tag_page_id IN \
                     (SELECT id FROM pages WHERE name IN ({ph})))"
                )
            }
            Query::AllPageTags => "p.id IN (SELECT tag_page_id FROM page_tags)".to_owned(),
            _ => {
                return Err(QueryError::Syntax(
                    "block-level filter in a page query".to_owned(),
                ));
            }
        })
    }

    fn order_by(&mut self, sort: &SortBy, blocks: bool) -> String {
        let dir = if sort.desc { "DESC" } else { "ASC" };
        let (idc, tbl, ptbl) = if blocks {
            ("b.id", "block_property_values", "block_properties")
        } else {
            ("p.id", "page_property_values", "page_properties")
        };
        let owner = if blocks { "block_id" } else { "page_id" };
        let mut num =
            format!("(SELECT MIN(v.value_num) FROM {tbl} v WHERE v.{owner} = {idc} AND v.key = ?)");
        if blocks {
            let col = match sort.key.as_str() {
                "created-at" => Some("created_at"),
                "last-modified-at" => Some("updated_at"),
                _ => None,
            };
            if let Some(c) = col {
                num = format!("COALESCE({num}, b.{c})");
            }
        }
        let raw = format!(
            "(SELECT MIN(x.raw_value) FROM {ptbl} x WHERE x.{owner} = {idc} AND x.key = ?)"
        );
        // `num` appears three times and `raw` twice: five bindings of the key, in text order.
        for _ in 0..5 {
            self.p(text(&sort.key));
        }
        format!("({num} IS NULL AND {raw} IS NULL), {num} IS NULL, {num} {dir}, {raw} {dir}")
    }
}

/// SQL for one leaf filter over alias `b` (blocks) or `p` (pages), with its parameters. Used
/// by the advanced compiler to inline the DSL rules.
pub(super) fn leaf_sql(
    q: &Query,
    ctx: &QueryContext,
    blocks: bool,
) -> Result<(String, Vec<Value>), QueryError> {
    let mut g = Gen {
        ctx,
        params: Vec::new(),
        blocks,
    };
    let sql = if blocks {
        g.block_leaf(q)?
    } else {
        g.page_leaf(q)?
    };
    Ok((sql, g.params))
}

/// Compiles a parsed query.
pub fn compile(q: &SimpleQuery, ctx: &QueryContext) -> Result<Compiled, QueryError> {
    let blocks = q.filter.returns_blocks();
    let mut g = Gen {
        ctx,
        params: Vec::new(),
        blocks,
    };
    let where_sql = g.expr(&q.filter)?;
    let mut sql = if blocks {
        format!("SELECT {BLOCK_COLS} FROM blocks b WHERE {where_sql}")
    } else {
        format!("SELECT {PAGE_COLS} FROM pages p WHERE {where_sql}")
    };
    if blocks && let Some(uuid) = &ctx.query_block {
        sql.push_str(" AND b.uuid <> ?");
        g.p(text(uuid));
    }
    if let Some(n) = q.sample {
        sql.push_str(" ORDER BY random() LIMIT ?");
        g.p(Value::Integer(i64::try_from(n).unwrap_or(i64::MAX)));
    } else {
        let order = match &q.sort_by {
            Some(s) => g.order_by(s, blocks),
            None => String::new(),
        };
        let base = if blocks {
            "b.page_id, b.file_id, b.ord"
        } else {
            "p.name"
        };
        sql.push_str(" ORDER BY ");
        if !order.is_empty() {
            sql.push_str(&order);
            sql.push_str(", ");
        }
        sql.push_str(base);
        if let Some(n) = ctx.limit {
            sql.push_str(" LIMIT ?");
            g.p(Value::Integer(i64::try_from(n).unwrap_or(i64::MAX)));
        }
    }
    Ok(Compiled {
        sql,
        params: g.params,
        kind: if blocks {
            ResultKind::Blocks
        } else {
            ResultKind::Pages
        },
    })
}
