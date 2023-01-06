use itertools::Itertools;
use std::{collections::HashSet, io::Write};

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

impl Map {
    pub fn shortest_path(&self, gps_start: &Node, street: &str) -> Vec<Node> {
        let starting_node = self.find_starting_node(gps_start);
        let end_node = self.find_ending_node(gps_start, street);
        let path = self.greedy_path(&starting_node, &end_node);
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

    fn greedy_path(&self, start: &GNode, end: &GNode) -> Vec<Node> {
        let mut stack = vec![([*start, *start], 0)];
        let mut path = Vec::new();
        let mut seen_nodes = HashSet::new(); // NOTE: this will be a BitVec and not a hashset
        while let Some((travel, depth)) = stack.pop() {
            while let Some((_, d)) = path.last() {
                // we backtrack, cancel
                if *d < depth {
                    break;
                } else {
                    path.pop();
                }
            }
            if seen_nodes.contains(&travel[1].id) {
                continue;
            }
            seen_nodes.insert(travel[0].id);
            seen_nodes.insert(travel[1].id);
            let current_node = travel[1];
            eprintln!("we are at {current_node:?}");
            path.push((current_node.node, depth));
            if current_node.is(end) {
                return path.into_iter().map(|(n, _)| n).collect();
            }
            stack.extend(
                self.neighbours(&current_node)
                    .sorted_by(|ta, tb| {
                        ta[1]
                            .squared_distance_between(end)
                            .partial_cmp(&tb[1].squared_distance_between(end))
                            .unwrap()
                    })
                    .map(|t| (t, depth + 1)),
            );
        }
        Vec::new() // no path found
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
                    eprintln!("yes to {nodes:?}");
                    Some([nodes[0], nodes[1]])
                } else if nodes[1].is(node) {
                    eprintln!("yes2 to {nodes:?}");
                    Some([nodes[1], nodes[0]])
                } else {
                    None
                }
            })
    }
}
