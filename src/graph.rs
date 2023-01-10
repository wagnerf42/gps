use itertools::Itertools;
use std::{
    collections::{BinaryHeap, HashMap, HashSet},
    io::Write,
};

use crate::{save_svg, CWayId, Map, Node, Svg, SvgW, WayId};

#[derive(Debug, Clone, Copy)]
struct GNode {
    id: usize,
    way_id: usize,
    node: Node,
}

impl<W: Write> Svg<W> for GNode {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        self.node.write_svg(writer, color)
    }
}

impl std::ops::Deref for GNode {
    type Target = Node;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl PartialEq for GNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for GNode {}

impl std::hash::Hash for GNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl Map {
    pub fn shortest_path(&self, gps_start: &Node, street: &str) -> Vec<Node> {
        let starting_node = self.find_starting_node(gps_start);
        let end_node = self.find_ending_node(gps_start, street);
        let greedy_path_length = self.greedy_path(&starting_node, &end_node);
        eprintln!("greedy path has length {greedy_path_length}");
        let path = self.low_level_a_star(&starting_node, &end_node, greedy_path_length);
        save_svg(
            "path.svg",
            self.bounding_box(),
            [
                self as SvgW,
                &starting_node as SvgW,
                &end_node as SvgW,
                &path as SvgW,
            ],
        )
        .unwrap();
        path
    }

    fn a_star(&self, start: &GNode, end: &GNode, greedy_path_length: f64) -> Vec<Node> {
        let mut heap = BinaryHeap::new();
        heap.push(HeapEntry {
            predecessor: None,
            travel: [*start, *start],
            distance: 0.,
        });

        let mut seen_nodes = HashSet::new(); // TODO: replace by bitvec
        let mut predecessors = HashMap::new(); // TODO: replace with vec and renumbering of nodes
        while let Some(entry) = heap.pop() {
            if seen_nodes.contains(&entry.travel[1].id) {
                continue;
            }
            seen_nodes.insert(entry.travel[0].id);
            seen_nodes.insert(entry.travel[1].id);
            let current_node = entry.travel[1];
            if let Some(predecessor) = entry.predecessor {
                predecessors.insert(entry.travel[1], predecessor);
            }
            if current_node.is(end) {
                return rebuild_path(&current_node, &predecessors);
            }
            heap.extend(
                self.neighbours(&current_node)
                    .map(|travel| HeapEntry {
                        predecessor: Some(current_node),
                        travel,
                        distance: entry.distance + self.way_length(travel[1].way_id),
                    })
                    .filter(|entry| {
                        entry.distance + entry.travel[1].squared_distance_between(end).sqrt()
                            < greedy_path_length
                    }),
            );
        }
        Vec::new()
    }

    fn low_level_a_star(&self, start: &GNode, end: &GNode, greedy_path_length: f64) -> Vec<Node> {
        let mut heap = BinaryHeap::new();
        heap.push(HeapEntry {
            predecessor: None,
            travel: [*start, *start],
            distance: 0.,
        });

        let mut seen_nodes = HashSet::new(); // TODO: replace by bitvec
        let mut predecessors = Vec::new();
        while let Some(entry) = heap.pop() {
            if seen_nodes.contains(&entry.travel[1].id) {
                continue;
            }
            seen_nodes.insert(entry.travel[0].id);
            seen_nodes.insert(entry.travel[1].id);
            let current_node = entry.travel[1];
            if let Some(predecessor) = entry.predecessor {
                predecessors.push((entry.travel[1], predecessor));
            }
            if current_node.is(end) {
                return rebuild_path_vec(&current_node, &predecessors);
            }
            heap.extend(
                self.neighbours(&current_node)
                    .map(|travel| HeapEntry {
                        predecessor: Some(current_node),
                        travel,
                        distance: entry.distance + self.way_length(travel[1].way_id),
                    })
                    .filter(|entry| {
                        entry.distance + entry.travel[1].squared_distance_between(end).sqrt()
                            < greedy_path_length
                    }),
            );
        }
        Vec::new()
    }

    fn greedy_path(&self, start: &GNode, end: &GNode) -> f64 {
        let mut heap = BinaryHeap::new();
        heap.push(HeapEntry {
            predecessor: None,
            travel: [*start, *start],
            distance: start.squared_distance_between(end),
        });
        let mut seen_nodes = HashSet::new(); // TODO: replace by bitvec
        let mut predecessors = HashMap::new(); // TODO: replace with vec and renumbering of nodes
        while let Some(entry) = heap.pop() {
            if seen_nodes.contains(&entry.travel[1].id) {
                continue;
            }
            seen_nodes.insert(entry.travel[0].id);
            seen_nodes.insert(entry.travel[1].id);
            let current_node = entry.travel[1];
            if let Some(predecessor) = entry.predecessor {
                predecessors.insert(entry.travel[1], predecessor);
            }
            if current_node.is(end) {
                return self.path_length(&current_node, &predecessors);
                // return rebuild_path(&current_node, start, &predecessors);
            }

            heap.extend(self.neighbours(&current_node).map(|travel| HeapEntry {
                predecessor: Some(current_node),
                travel,
                distance: travel[1].squared_distance_between(end),
            }));
        }
        0.
    }

    fn connected_component(&self, start: &GNode) -> Vec<Vec<Node>> {
        let mut stack = vec![[*start, *start]];
        let mut component = Vec::new();
        let mut seen_nodes = HashSet::new(); // NOTE: this will be a BitVec and not a hashset
        while let Some(travel) = stack.pop() {
            if seen_nodes.contains(&travel[1].id) {
                continue;
            }
            seen_nodes.insert(travel[0].id);
            seen_nodes.insert(travel[1].id);
            let current_node = travel[1];
            let edge = travel.iter().map(|n| n.node).collect::<Vec<_>>();
            if edge[0] != edge[1] {
                component.push(edge);
            }
            stack.extend(self.neighbours(&current_node));
        }
        component
    }

    // go to nearest node in the street
    fn find_ending_node(&self, gps_start: &Node, street: &str) -> GNode {
        self.streets
            .get(street)
            .into_iter()
            .flatten()
            .flat_map(|&way_id| {
                let (way_offset, nodes) = self.way(way_id);
                nodes
                    .first()
                    .map(|n| GNode {
                        id: way_offset,
                        way_id: way_offset,
                        node: *n,
                    })
                    .into_iter()
                    .chain(nodes.last().map(|n| GNode {
                        id: way_offset + 1,
                        way_id: way_offset,
                        node: *n,
                    }))
            })
            .min_by(|na, nb| {
                na.squared_distance_between(gps_start)
                    .partial_cmp(&nb.squared_distance_between(gps_start))
                    .unwrap()
            })
            .unwrap()
    }

    fn tile_edges(&self, tile_x: usize, tile_y: usize) -> impl Iterator<Item = [GNode; 2]> + '_ {
        self.tile_ways_ends(tile_x, tile_y)
            .map(|(way_offset, nodes)| {
                [
                    GNode {
                        id: way_offset,
                        way_id: way_offset,
                        node: nodes[0],
                    },
                    GNode {
                        id: way_offset + 1,
                        way_id: way_offset,
                        node: nodes[1],
                    },
                ]
            })
    }

    fn find_starting_node(&self, gps_start: &Node) -> GNode {
        //TODO: fixme if between tiles
        //TODO: fixme if outside of grid
        //TODO: fixme if empty tile
        let (tile_x, tile_y) = self.node_tiles(gps_start).next().unwrap();
        //TODO: tile_ways
        self.tile_edges(tile_x, tile_y)
            .flatten()
            .min_by(|na, nb| {
                na.squared_distance_between(gps_start)
                    .partial_cmp(&nb.squared_distance_between(gps_start))
                    .unwrap()
            })
            .unwrap()
    }

    // this is tough.
    // if we have two ways connecting, let's say w1 = (s1, e1) and w2 = (s2, e2) :
    // such that e1 is s2.
    // if we are located at e1 : we can go at s1
    // but we can also go at e2 : however doing so we need to mark both s2 and e2 as visited
    // and not only e2.
    // that's why i cannot loop on the neighbours only, i also need the intermediate points.
    fn neighbours<'a>(&'a self, node: &'a GNode) -> impl Iterator<Item = [GNode; 2]> + 'a {
        self.node_tiles(node)
            .flat_map(|(tile_x, tile_y)| self.tile_edges(tile_x, tile_y))
            .filter_map(|nodes| {
                if nodes[0].is(node) {
                    Some([nodes[0], nodes[1]])
                } else if nodes[1].is(node) {
                    Some([nodes[1], nodes[0]])
                } else {
                    None
                }
            })
    }
    fn path_length(&self, end: &GNode, predecessors: &HashMap<GNode, GNode>) -> f64 {
        std::iter::successors(Some(*end), |current_node| {
            predecessors.get(current_node).copied()
        })
        .map(|node| node.way_id)
        .dedup()
        .map(|way_id| self.way_length(way_id))
        .sum::<f64>()
    }
}

fn rebuild_path(end: &GNode, predecessors: &HashMap<GNode, GNode>) -> Vec<Node> {
    std::iter::successors(Some(*end), |current_node| {
        predecessors.get(current_node).copied()
    })
    .map(|n| n.node)
    .collect()
}

fn rebuild_path_vec(end: &GNode, predecessors: &[(GNode, GNode)]) -> Vec<Node> {
    predecessors
        .iter()
        .rev()
        .fold(vec![end.node], |mut path, (na, prec_na)| {
            let current_node = path.last().unwrap();
            if na.node.is(current_node) {
                path.push(prec_na.node)
            }
            path
        })
}

struct HeapEntry {
    predecessor: Option<GNode>,
    travel: [GNode; 2],
    distance: f64,
}
impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}
impl Eq for HeapEntry {}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.distance
            .partial_cmp(&other.distance)
            .map(std::cmp::Ordering::reverse)
    }
}
impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}
