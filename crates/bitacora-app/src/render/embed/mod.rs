//! Embed guards (BIT-US-0104): cycle detection and the nesting depth limit.
//!
//! An embed is drawn inside the page (or inside another embed) that holds it. The chain lists
//! what is being drawn around the embed: the host page and every embed above it. Showing a
//! target that is already in the chain would recurse forever, and a deep chain would stall the
//! view, so both are refused with a message instead.

use crate::render::widget::EmbedTarget;

/// Default maximum nesting of embeds.
pub const DEFAULT_MAX_DEPTH: usize = 5;

/// What stands between the page and the embed being drawn.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chain {
    idents: Vec<String>,
    depth: usize,
}

/// Why an embed is not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The target is already being drawn above this embed.
    Circular,
    /// Embeds are nested deeper than the limit.
    TooDeep,
}

impl Chain {
    /// The chain of a page: nothing but the page itself.
    pub fn page(title: &str) -> Self {
        Self {
            idents: vec![EmbedTarget::Page(title.to_owned()).ident()],
            depth: 0,
        }
    }

    /// The chain of a block shown on its own (a zoomed block or a block embed host).
    #[must_use]
    pub fn with_ident(mut self, ident: String) -> Self {
        if !self.idents.contains(&ident) {
            self.idents.push(ident);
        }
        self
    }

    /// Embeds above this point.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Whether `target` may be drawn here. `ancestors` are extra identities known to enclose
    /// the host block (its ancestors' `block:<uuid>` idents), so that embedding an ancestor is
    /// refused at once rather than one level down.
    pub fn check(
        &self,
        target: &EmbedTarget,
        ancestors: &[String],
        max_depth: usize,
    ) -> Result<(), Refusal> {
        let id = target.ident();
        if self.idents.contains(&id) || ancestors.contains(&id) {
            return Err(Refusal::Circular);
        }
        if self.depth >= max_depth {
            return Err(Refusal::TooDeep);
        }
        Ok(())
    }

    /// The chain inside an embed of `target` whose content lives on `page` (a block embed
    /// also encloses its page, so that embedding the whole page again is a cycle).
    #[must_use]
    pub fn enter(&self, target: &EmbedTarget, page: Option<&str>) -> Self {
        let mut next = self.clone();
        next.depth += 1;
        next.idents.push(target.ident());
        if let Some(page) = page {
            let p = EmbedTarget::Page(page.to_owned()).ident();
            if matches!(target, EmbedTarget::Block(_)) && !next.idents.contains(&p) {
                // Only the page of the block is enclosed when the block is embedded from
                // elsewhere; the enclosing page stays allowed for sibling blocks, see `check`.
                next.idents.push(p);
            }
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(n: u8) -> EmbedTarget {
        EmbedTarget::Block(format!("00000000-0000-4000-8000-{n:012}"))
    }

    #[test]
    fn embedding_the_host_page_is_circular_case_insensitively() {
        let chain = Chain::page("My Page");
        assert_eq!(
            chain.check(&EmbedTarget::Page("my page".into()), &[], 5),
            Err(Refusal::Circular)
        );
        assert_eq!(
            chain.check(&EmbedTarget::Page("Other".into()), &[], 5),
            Ok(())
        );
    }

    #[test]
    fn embedding_an_ancestor_block_is_circular() {
        let chain = Chain::page("P");
        let anc = vec![block(1).ident()];
        assert_eq!(chain.check(&block(1), &anc, 5), Err(Refusal::Circular));
        assert_eq!(chain.check(&block(2), &anc, 5), Ok(()));
    }

    #[test]
    fn nested_embeds_stop_at_the_depth_limit_and_at_repeats() {
        let mut chain = Chain::page("P");
        for n in 1..=3 {
            assert_eq!(chain.check(&block(n), &[], 3), Ok(()));
            chain = chain.enter(&block(n), None);
        }
        assert_eq!(chain.depth(), 3);
        assert_eq!(chain.check(&block(9), &[], 3), Err(Refusal::TooDeep));
        assert_eq!(chain.check(&block(2), &[], 9), Err(Refusal::Circular));
    }

    #[test]
    fn a_block_embed_also_encloses_its_page() {
        let chain = Chain::page("Host").enter(&block(1), Some("Source"));
        assert_eq!(
            chain.check(&EmbedTarget::Page("source".into()), &[], 5),
            Err(Refusal::Circular)
        );
    }
}
