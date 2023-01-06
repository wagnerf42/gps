use itertools::Itertools;
use std::collections::HashSet;

use crate::{save_svg, CWayId, Map, Node, SvgW, WayId};

impl Map {
    pub fn shortest_path(&self, gps_start: &Node, street: &str) -> Vec<Node> {
        let starting_node = self.find_starting_node(gps_start);
        let end_node = self.find_ending_node(gps_start, street);
        save_svg(
            "path.svg",
            self.bounding_box(),
            [self as SvgW, &starting_node as SvgW, &end_node as SvgW],
        )
        .unwrap();
        let path = self.greedy_path(&starting_node, &end_node);
        path
    }

    fn greedy_path(&self, start: &Node, end: &Node) -> Vec<Node> {
        //TODO: avoid seing same node twice
        let mut stack = vec![(*start, 0)];
        let mut path = Vec::new();
        while let Some((current, depth)) = stack.pop() {
            while let Some((_, d)) = path.last() {
                // we backtrack, cancel
                if *d < depth {
                    break;
                } else {
                    path.pop();
                }
            }
            path.push((current, depth));
            if current.is(end) {
                return path.into_iter().map(|(n, _)| n).collect();
            }
            stack.extend(
                self.neighbours(&current)
                    .sorted_by(|na, nb| {
                        na.squared_distance_between(end)
                            .partial_cmp(&nb.squared_distance_between(end))
                            .unwrap()
                    })
                    .map(|n| (n, depth + 1)),
            );
        }
        Vec::new() // no path found
    }

    // go to nearest node in the street
    fn find_ending_node(&self, gps_start: &Node, street: &str) -> Node {
        self.streets
            .get(street)
            .into_iter()
            .flatten()
            .flat_map(|&way_id| self.way(way_id))
            .min_by(|na, nb| {
                na.squared_distance_between(gps_start)
                    .partial_cmp(&nb.squared_distance_between(gps_start))
                    .unwrap()
            })
            .unwrap()
    }

    fn find_starting_node(&self, gps_start: &Node) -> Node {
        //TODO: fixme if between tiles
        //TODO: fixme if outside of grid
        //TODO: fixme if empty tile
        let (tile_x, tile_y) = self.node_tiles(gps_start).next().unwrap();
        //TODO: tile_ways
        self.tile_ways_ends(tile_x, tile_y)
            .flatten()
            .min_by(|na, nb| {
                na.squared_distance_between(gps_start)
                    .partial_cmp(&nb.squared_distance_between(gps_start))
                    .unwrap()
            })
            .unwrap()
    }

    pub(crate) fn neighbours<'a>(&'a self, node: &'a Node) -> impl Iterator<Item = Node> + 'a {
        self.node_tiles(node)
            .flat_map(|(tile_x, tile_y)| self.tile_ways_ends(tile_x, tile_y))
            .filter_map(|nodes| {
                if nodes[0].is(node) {
                    Some(nodes[1])
                } else if nodes[1].is(node) {
                    Some(nodes[0])
                } else {
                    None
                }
            })
    }
}
