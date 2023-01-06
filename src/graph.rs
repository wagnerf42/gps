use itertools::Itertools;
use std::collections::HashSet;

use crate::{CWayId, Map, Node, WayId};

impl Map {
    pub fn shortest_path(&self, gps_start: &Node, street: &str) -> Vec<Node> {
        let (starting_way, starting_node) = self.find_starting_node(gps_start);
        let end_node = self.find_ending_node(gps_start, street);
        self.greedy_path(self.way(starting_way).first().unwrap(), &end_node)
    }

    fn greedy_path(&self, start: &Node, end: &Node) -> Vec<Node> {
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

    fn find_starting_node(&self, gps_start: &Node) -> (CWayId, Node) {
        //TODO: fixme if between tiles
        //TODO: fixme if outside of grid
        //TODO: fixme if empty tile
        let (tile_x, tile_y) = self.node_tiles(gps_start).next().unwrap();
        self.tile_ways(tile_x, tile_y)
            .enumerate()
            .flat_map(move |(way_id, way_nodes)| way_nodes.into_iter().map(move |n| (way_id, n)))
            .min_by(|(_, na), (_, nb)| {
                na.squared_distance_between(gps_start)
                    .partial_cmp(&nb.squared_distance_between(gps_start))
                    .unwrap()
            })
            .map(|(way_id, n)| {
                (
                    (
                        (tile_x + tile_y * self.grid_size.0) as u16,
                        way_id as u16,
                    ),
                    n,
                )
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
