//! The graph algorithms of the knowledge graph (D-60): the one place for the traversals that the
//! exporter, the queries, `impact` and the diagrams use. Each keeps an explicit stack or queue, so a
//! deep graph cannot exhaust the thread's stack, and each visits nodes in index order, so a caller
//! that numbers its nodes in the byte order of their IDs gets results in that order.

use std::collections::VecDeque;

/// A directed graph over the nodes `0..len()` in compressed sparse rows: the successors of `v` are
/// `targets[offsets[v]..offsets[v + 1]]`, ascending and without duplicates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Digraph {
    offsets: Vec<usize>,
    targets: Vec<usize>,
}

impl Digraph {
    /// The graph of `nodes` nodes with the given edges; duplicates collapse, and an edge whose end
    /// is not a node is dropped.
    pub fn from_edges(nodes: usize, edges: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut pairs: Vec<(usize, usize)> = edges
            .into_iter()
            .filter(|&(from, to)| from < nodes && to < nodes)
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        let mut offsets = vec![0; nodes + 1];
        for &(from, _) in &pairs {
            offsets[from + 1] += 1;
        }
        for node in 0..nodes {
            offsets[node + 1] += offsets[node];
        }
        Self {
            offsets,
            targets: pairs.into_iter().map(|(_, to)| to).collect(),
        }
    }

    /// The graph whose node `v` has the successors `lists[v]`.
    pub fn from_lists(lists: &[Vec<usize>]) -> Self {
        Self::from_edges(
            lists.len(),
            lists
                .iter()
                .enumerate()
                .flat_map(|(from, targets)| targets.iter().map(move |&to| (from, to))),
        )
    }

    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn edge_count(&self) -> usize {
        self.targets.len()
    }

    pub fn successors(&self, node: usize) -> &[usize] {
        &self.targets[self.offsets[node]..self.offsets[node + 1]]
    }

    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        from < self.len() && self.successors(from).binary_search(&to).is_ok()
    }

    /// The graph with every edge turned around.
    pub fn reversed(&self) -> Self {
        Self::from_edges(
            self.len(),
            (0..self.len())
                .flat_map(|from| self.successors(from).iter().map(move |&to| (to, from))),
        )
    }
}

/// A node that a breadth-first search reached: the number of edges from the nearest start node,
/// and the node of the previous layer that reached it first (`None` for a start node).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visit {
    pub node: usize,
    pub depth: usize,
    pub parent: Option<usize>,
}

/// Breadth-first search, one layer at a time: the start nodes are layer 0, and layer `d + 1` holds
/// the unvisited successors of layer `d`, which is taken in ascending node order (so a node's
/// parent is the least node of the previous layer that reaches it). Stops after layer `depth` or
/// when `limit` nodes beyond the start nodes were reached. The visits are in the order of
/// discovery: by layer, then by parent, then by node.
pub fn bfs(graph: &Digraph, start: &[usize], depth: usize, limit: usize) -> Vec<Visit> {
    let mut seen = vec![false; graph.len()];
    let mut visits = Vec::new();
    let mut layer: Vec<usize> = Vec::new();
    for &node in start {
        if node < graph.len() && !seen[node] {
            seen[node] = true;
            layer.push(node);
            visits.push(Visit {
                node,
                depth: 0,
                parent: None,
            });
        }
    }
    let mut reached = 0;
    for level in 1..=depth {
        layer.sort_unstable();
        let mut next = Vec::new();
        for &from in &layer {
            for &to in graph.successors(from) {
                if seen[to] {
                    continue;
                }
                if reached == limit {
                    return visits;
                }
                seen[to] = true;
                reached += 1;
                next.push(to);
                visits.push(Visit {
                    node: to,
                    depth: level,
                    parent: Some(from),
                });
            }
        }
        if next.is_empty() {
            break;
        }
        layer = next;
    }
    visits
}

/// Whether the breadth-first search of `bfs` with this `depth` leaves a node unvisited one layer
/// further: some node of layer `depth` has an unvisited successor.
pub fn reaches_beyond(graph: &Digraph, start: &[usize], depth: usize) -> bool {
    let visits = bfs(graph, start, depth, usize::MAX);
    let mut seen = vec![false; graph.len()];
    for visit in &visits {
        seen[visit.node] = true;
    }
    visits
        .iter()
        .filter(|visit| visit.depth == depth)
        .any(|visit| graph.successors(visit.node).iter().any(|&to| !seen[to]))
}

/// A path with the fewest edges from `from` to `to` within `depth` edges, as its nodes from `from`
/// to `to`. Among the shortest paths it is the one that `bfs` finds: each node's parent is the
/// least node of the previous layer.
pub fn shortest_path(graph: &Digraph, from: usize, to: usize, depth: usize) -> Option<Vec<usize>> {
    if from >= graph.len() || to >= graph.len() {
        return None;
    }
    if from == to {
        return Some(vec![from]);
    }
    let mut parent: Vec<Option<usize>> = vec![None; graph.len()];
    let mut seen = vec![false; graph.len()];
    seen[from] = true;
    let mut layer = vec![from];
    for _ in 0..depth {
        layer.sort_unstable();
        let mut next = Vec::new();
        for &node in &layer {
            for &successor in graph.successors(node) {
                if seen[successor] {
                    continue;
                }
                seen[successor] = true;
                parent[successor] = Some(node);
                if successor == to {
                    let mut path = vec![to];
                    let mut at = to;
                    while let Some(previous) = parent[at] {
                        path.push(previous);
                        at = previous;
                    }
                    path.reverse();
                    return Some(path);
                }
                next.push(successor);
            }
        }
        if next.is_empty() {
            return None;
        }
        layer = next;
    }
    None
}

/// The strongly connected components (Tarjan's algorithm with an explicit stack): the component
/// of each node, numbered in the order the components complete, which is a reverse topological
/// order of the condensation (a component's successors complete first).
pub fn strongly_connected(graph: &Digraph) -> Vec<usize> {
    const UNVISITED: usize = usize::MAX;
    let count = graph.len();
    let mut index = vec![UNVISITED; count];
    let mut low = vec![0; count];
    let mut on_stack = vec![false; count];
    let mut component = vec![UNVISITED; count];
    let mut stack = Vec::new();
    let mut next_index = 0;
    let mut components = 0;
    // The frames of the depth-first search: a node and the position of its next successor.
    let mut frames: Vec<(usize, usize)> = Vec::new();
    for root in 0..count {
        if index[root] != UNVISITED {
            continue;
        }
        frames.push((root, 0));
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (node, ref mut position)) = frames.last_mut() {
            let successors = graph.successors(node);
            if let Some(&successor) = successors.get(*position) {
                *position += 1;
                if index[successor] == UNVISITED {
                    index[successor] = next_index;
                    low[successor] = next_index;
                    next_index += 1;
                    stack.push(successor);
                    on_stack[successor] = true;
                    frames.push((successor, 0));
                } else if on_stack[successor] {
                    low[node] = low[node].min(index[successor]);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component[member] = components;
                    if member == node {
                        break;
                    }
                }
                components += 1;
            }
        }
    }
    component
}

/// The members of each strongly connected component, by the numbering of `strongly_connected`,
/// each ascending.
pub fn components(of: &[usize]) -> Vec<Vec<usize>> {
    let count = of.iter().copied().max().map_or(0, |last| last + 1);
    let mut members = vec![Vec::new(); count];
    for (node, &component) in of.iter().enumerate() {
        members[component].push(node);
    }
    members
}

/// The nodes that reach a node of `targets` by at most `depth` edges, with the number of edges
/// and the next node on a shortest way there: `bfs` over the reversed graph.
pub fn reverse_reach(graph: &Digraph, targets: &[usize], depth: usize) -> Vec<Visit> {
    bfs(&graph.reversed(), targets, depth, usize::MAX)
}

/// The nodes in an order where every edge goes from an earlier node to a later one, if the graph
/// has no cycle (Kahn's algorithm, taking the least ready node first).
pub fn topological_order(graph: &Digraph) -> Option<Vec<usize>> {
    let mut indegree = vec![0usize; graph.len()];
    for node in 0..graph.len() {
        for &to in graph.successors(node) {
            indegree[to] += 1;
        }
    }
    let mut ready: std::collections::BTreeSet<usize> = (0..graph.len())
        .filter(|&node| indegree[node] == 0)
        .collect();
    let mut order = Vec::with_capacity(graph.len());
    while let Some(node) = ready.pop_first() {
        order.push(node);
        for &to in graph.successors(node) {
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.insert(to);
            }
        }
    }
    (order.len() == graph.len()).then_some(order)
}

/// Whether `parents` (the parent of each node, if any) is a forest: following parents from any
/// node ends at a root without coming back. Returns the first node on a cycle otherwise.
pub fn forest_cycle(parents: &[Option<usize>]) -> Option<usize> {
    // 0 = not seen, 1 = on the current walk, 2 = known to end at a root.
    let mut state = vec![0u8; parents.len()];
    let mut walk = VecDeque::new();
    for start in 0..parents.len() {
        let mut node = start;
        while state[node] == 0 {
            state[node] = 1;
            walk.push_back(node);
            match parents[node] {
                Some(parent) if parent < parents.len() => node = parent,
                _ => break,
            }
        }
        if state[node] == 1 && parents[node].is_some_and(|parent| parent < parents.len()) {
            return Some(node);
        }
        for node in walk.drain(..) {
            state[node] = 2;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(nodes: usize, edges: &[(usize, usize)]) -> Digraph {
        Digraph::from_edges(nodes, edges.iter().copied())
    }

    #[test]
    fn builds_sorted_unique_rows() {
        let g = graph(4, &[(0, 2), (0, 1), (0, 2), (3, 0), (1, 9)]);
        assert_eq!(g.successors(0), &[1, 2]);
        assert_eq!(g.successors(1), &[] as &[usize]);
        assert_eq!(g.successors(3), &[0]);
        assert_eq!(g.edge_count(), 3);
        assert!(g.has_edge(3, 0) && !g.has_edge(0, 3));
        assert_eq!(g.reversed().successors(0), &[3]);
    }

    #[test]
    fn bfs_is_layered_and_bounded() {
        // 0 -> 1 -> 2 -> 3 -> 4, and 0 -> 3.
        let g = graph(5, &[(0, 1), (1, 2), (2, 3), (3, 4), (0, 3)]);
        let visits = bfs(&g, &[0], 2, usize::MAX);
        let found: Vec<_> = visits.iter().map(|v| (v.node, v.depth, v.parent)).collect();
        assert_eq!(
            found,
            [
                (0, 0, None),
                (1, 1, Some(0)),
                (3, 1, Some(0)),
                (2, 2, Some(1)),
                (4, 2, Some(3))
            ]
        );
        assert_eq!(bfs(&g, &[0], 8, 2).len(), 3);
        assert!(reaches_beyond(&graph(3, &[(0, 1), (1, 2)]), &[0], 1));
        assert!(!reaches_beyond(&graph(3, &[(0, 1), (1, 2)]), &[0], 2));
    }

    #[test]
    fn shortest_path_prefers_the_least_parent() {
        // Two paths of length 2 from 0 to 3: through 1 and through 2.
        let g = graph(4, &[(0, 2), (0, 1), (1, 3), (2, 3)]);
        assert_eq!(shortest_path(&g, 0, 3, 8), Some(vec![0, 1, 3]));
        assert_eq!(shortest_path(&g, 0, 3, 1), None);
        assert_eq!(shortest_path(&g, 3, 0, 8), None);
        assert_eq!(shortest_path(&g, 2, 2, 0), Some(vec![2]));
    }

    #[test]
    fn strongly_connected_components_are_exact() {
        // {0, 1, 2} is a cycle, 3 has a self loop, 4 is alone, 2 -> 3 -> 4.
        let g = graph(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 3), (3, 4)]);
        let of = strongly_connected(&g);
        assert_eq!(of[0], of[1]);
        assert_eq!(of[1], of[2]);
        assert_ne!(of[2], of[3]);
        assert_ne!(of[3], of[4]);
        // Reverse topological: 4 completes before 3, which completes before {0, 1, 2}.
        assert!(of[4] < of[3] && of[3] < of[0]);
        let members = components(&of);
        assert_eq!(members[of[0]], [0, 1, 2]);
    }

    #[test]
    fn deep_chains_do_not_recurse() {
        let count = 200_000;
        let g = graph(
            count,
            &(0..count - 1).map(|n| (n, n + 1)).collect::<Vec<_>>(),
        );
        let of = strongly_connected(&g);
        assert_eq!(components(&of).len(), count);
        assert_eq!(topological_order(&g).map(|order| order.len()), Some(count));
        let mut cyclic: Vec<_> = (0..count - 1).map(|n| (n, n + 1)).collect();
        cyclic.push((count - 1, 0));
        let cycle = graph(count, &cyclic);
        assert_eq!(components(&strongly_connected(&cycle)).len(), 1);
        assert_eq!(topological_order(&cycle), None);
    }

    #[test]
    fn reverse_reach_and_forests() {
        let g = graph(4, &[(0, 1), (1, 2), (3, 2)]);
        let reached: Vec<_> = reverse_reach(&g, &[2], 1).iter().map(|v| v.node).collect();
        assert_eq!(reached, [2, 1, 3]);
        assert_eq!(forest_cycle(&[None, Some(0), Some(0), Some(2)]), None);
        assert!(forest_cycle(&[Some(1), Some(0), None]).is_some());
    }
}
