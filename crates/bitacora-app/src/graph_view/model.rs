//! Pure graph-view model: index [`GraphData`] converted to layout input, neighbour lookups,
//! N-hop focus subsets and position carry-over for incremental refreshes (BIT-SP-0012.R2/R6).
//!
//! No GPUI types here, so everything is unit-testable.

use std::collections::HashMap;

use bitacora_graph::GraphInput;
use bitacora_index::GraphData;

/// Node radius in layout units: `8 * max(1, cbrt(degree))` (Logseq's graph-view sizing).
#[must_use]
pub fn node_radius(degree: u32) -> f32 {
    8.0 * (degree as f32).cbrt().max(1.0)
}

/// One visible page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInfo {
    /// Page id in the index.
    pub id: i64,
    /// Display name.
    pub name: String,
    /// Journal page.
    pub is_journal: bool,
    /// Used as a tag.
    pub is_tag: bool,
    /// Edges incident to the node among the visible nodes.
    pub degree: u32,
}

impl NodeInfo {
    /// Radius in layout units.
    #[must_use]
    pub fn radius(&self) -> f32 {
        node_radius(self.degree)
    }
}

/// The nodes and undirected links of one layout, with an adjacency list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphModel {
    nodes: Vec<NodeInfo>,
    links: Vec<(u32, u32)>,
    index: HashMap<i64, usize>,
    adjacency: Vec<Vec<u32>>,
}

impl GraphModel {
    /// Builds the model from index data (edges naming unknown pages are dropped).
    #[must_use]
    pub fn from_data(data: &GraphData) -> Self {
        let nodes = data
            .nodes
            .iter()
            .map(|n| NodeInfo {
                id: n.id,
                name: n.name.clone(),
                is_journal: n.is_journal,
                is_tag: n.is_tag,
                degree: n.degree,
            })
            .collect();
        let links = {
            let index: HashMap<i64, usize> = data
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id, i))
                .collect();
            data.edges
                .iter()
                .filter_map(|e| {
                    let (a, b) = (*index.get(&e.src)?, *index.get(&e.dst)?);
                    (a != b).then_some((a as u32, b as u32))
                })
                .collect()
        };
        Self::new(nodes, links)
    }

    fn new(nodes: Vec<NodeInfo>, links: Vec<(u32, u32)>) -> Self {
        let index = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let mut adjacency = vec![Vec::new(); nodes.len()];
        for &(a, b) in &links {
            adjacency[a as usize].push(b);
            adjacency[b as usize].push(a);
        }
        Self {
            nodes,
            links,
            index,
            adjacency,
        }
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// No nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The nodes, in layout order.
    #[must_use]
    pub fn nodes(&self) -> &[NodeInfo] {
        &self.nodes
    }

    /// Undirected links as node index pairs.
    #[must_use]
    pub fn links(&self) -> &[(u32, u32)] {
        &self.links
    }

    /// Layout index of a page id.
    #[must_use]
    pub fn index_of(&self, id: i64) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// Neighbours of node `ix`.
    #[must_use]
    pub fn neighbours(&self, ix: usize) -> &[u32] {
        self.adjacency.get(ix).map_or(&[], Vec::as_slice)
    }

    /// Input for the layout engine.
    #[must_use]
    pub fn to_input(&self) -> GraphInput {
        GraphInput {
            node_count: self.nodes.len(),
            links: self.links.clone(),
        }
    }

    /// Page ids within `hops` links of any of `focus` (the focus nodes themselves included).
    /// Unknown focus ids are ignored; an empty result means nothing to focus on.
    #[must_use]
    pub fn within_hops(&self, focus: &[i64], hops: u32) -> Vec<i64> {
        let mut dist: Vec<Option<u32>> = vec![None; self.nodes.len()];
        let mut frontier: Vec<usize> = focus.iter().filter_map(|id| self.index_of(*id)).collect();
        for &ix in &frontier {
            dist[ix] = Some(0);
        }
        for step in 1..=hops {
            let mut next = Vec::new();
            for &ix in &frontier {
                for &nb in self.neighbours(ix) {
                    let nb = nb as usize;
                    if dist[nb].is_none() {
                        dist[nb] = Some(step);
                        next.push(nb);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        self.nodes
            .iter()
            .zip(&dist)
            .filter_map(|(n, d)| d.map(|_| n.id))
            .collect()
    }

    /// The sub-model with only the pages in `keep` (links between kept pages survive; degrees
    /// are recomputed so node sizes follow what is visible).
    #[must_use]
    pub fn restrict(&self, keep: &[i64]) -> Self {
        let keep: std::collections::HashSet<i64> = keep.iter().copied().collect();
        let mut remap = vec![None; self.nodes.len()];
        let mut nodes = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if keep.contains(&n.id) {
                remap[i] = Some(nodes.len() as u32);
                nodes.push(NodeInfo {
                    degree: 0,
                    ..n.clone()
                });
            }
        }
        let links: Vec<(u32, u32)> = self
            .links
            .iter()
            .filter_map(|&(a, b)| Some((remap[a as usize]?, remap[b as usize]?)))
            .collect();
        for &(a, b) in &links {
            nodes[a as usize].degree += 1;
            nodes[b as usize].degree += 1;
        }
        Self::new(nodes, links)
    }
}

/// Positions for `new` carried over from `old` (BIT-SP-0012.R6): surviving pages keep their
/// place; a new page starts a short, deterministic offset away from an already placed
/// neighbour; a page with no placed neighbour gets `None` (the engine seeds it).
#[must_use]
pub fn carry_positions(
    old: &GraphModel,
    old_positions: &[[f32; 2]],
    new: &GraphModel,
) -> Vec<Option<[f32; 2]>> {
    let mut out: Vec<Option<[f32; 2]>> = new
        .nodes()
        .iter()
        .map(|n| {
            old.index_of(n.id)
                .and_then(|i| old_positions.get(i))
                .copied()
        })
        .collect();
    let placed = out.clone();
    for (i, slot) in out.iter_mut().enumerate() {
        if slot.is_some() {
            continue;
        }
        let anchor = new
            .neighbours(i)
            .iter()
            .find_map(|&nb| placed.get(nb as usize).copied().flatten());
        if let Some(at) = anchor {
            // Deterministic spread: the angle comes from the page id.
            let angle =
                (new.nodes()[i].id.unsigned_abs() % 360) as f32 * std::f32::consts::TAU / 360.0;
            *slot = Some([at[0] + 30.0 * angle.cos(), at[1] + 30.0 * angle.sin()]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use bitacora_index::{GraphDataEdge, GraphDataNode};

    use super::*;

    fn data(ids: &[i64], edges: &[(i64, i64)]) -> GraphData {
        let nodes = ids
            .iter()
            .map(|&id| GraphDataNode {
                id,
                name: format!("p{id}"),
                is_journal: false,
                is_tag: false,
                is_namespace_parent: false,
                degree: edges.iter().filter(|e| e.0 == id || e.1 == id).count() as u32,
            })
            .collect();
        GraphData {
            nodes,
            edges: edges
                .iter()
                .map(|&(src, dst)| GraphDataEdge { src, dst })
                .collect(),
        }
    }

    #[test]
    fn radius_follows_the_logseq_formula() {
        assert!((node_radius(0) - 8.0).abs() < 1e-6);
        assert!((node_radius(1) - 8.0).abs() < 1e-6);
        assert!((node_radius(8) - 16.0).abs() < 1e-4);
        assert!((node_radius(27) - 24.0).abs() < 1e-4);
    }

    #[test]
    fn conversion_maps_ids_to_indices_and_drops_dangling_edges() {
        let m = GraphModel::from_data(&data(&[10, 20, 30], &[(10, 20), (20, 30), (30, 99)]));
        assert_eq!(m.len(), 3);
        assert_eq!(m.links(), &[(0, 1), (1, 2)]);
        assert_eq!(m.to_input().node_count, 3);
        assert_eq!(m.neighbours(1), &[0, 2]);
        assert_eq!(m.index_of(30), Some(2));
    }

    #[test]
    fn hops_expand_from_focus_nodes() {
        let m = GraphModel::from_data(&data(&[1, 2, 3, 4, 5], &[(1, 2), (2, 3), (3, 4)]));
        assert_eq!(m.within_hops(&[1], 0), vec![1]);
        assert_eq!(m.within_hops(&[1], 1), vec![1, 2]);
        assert_eq!(m.within_hops(&[1], 2), vec![1, 2, 3]);
        assert_eq!(m.within_hops(&[1, 4], 1), vec![1, 2, 3, 4]);
        assert!(m.within_hops(&[42], 3).is_empty());
    }

    #[test]
    fn restrict_recomputes_degrees_and_links() {
        let m = GraphModel::from_data(&data(&[1, 2, 3], &[(1, 2), (2, 3)]));
        let r = m.restrict(&[2, 3]);
        assert_eq!(r.len(), 2);
        assert_eq!(r.links(), &[(0, 1)]);
        assert_eq!(r.nodes()[0].degree, 1);
    }

    #[test]
    fn carry_keeps_survivors_and_seeds_new_nodes_near_a_neighbour() {
        let old = GraphModel::from_data(&data(&[1, 2], &[(1, 2)]));
        let pos = [[100.0, 100.0], [-50.0, 20.0]];
        let new = GraphModel::from_data(&data(&[2, 3, 4], &[(2, 3)]));
        let carried = carry_positions(&old, &pos, &new);
        assert_eq!(carried[0], Some([-50.0, 20.0]));
        let seeded = carried[1].expect("new node next to a placed neighbour");
        let d = ((seeded[0] + 50.0).powi(2) + (seeded[1] - 20.0).powi(2)).sqrt();
        assert!((d - 30.0).abs() < 1e-3, "distance {d}");
        assert_eq!(carried[2], None);
        // Deterministic.
        assert_eq!(carried, carry_positions(&old, &pos, &new));
    }
}
