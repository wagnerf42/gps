use itertools::Itertools;
use std::collections::HashMap;

use crate::{CWayId, Node, NodeId, TileKey, WayId};

pub struct Map {
    pub binary_ways: Vec<u8>,
    pub start_coordinates: (f64, f64),
    pub tiles_sizes_prefix: Vec<usize>,
    pub tiles_per_line: usize,
    pub side: f64,
    pub streets: HashMap<String, Vec<CWayId>>,
}

impl Map {
    pub fn new(
        nodes: &[Node],
        ways: &[Vec<NodeId>],
        streets: HashMap<String, Vec<WayId>>,
        tiles: &HashMap<TileKey, Vec<WayId>>,
        side: f64,
    ) -> Self {
        let mut binary_ways = Vec::new();
        let mut tiles_sizes_prefix = Vec::new();
        let mut ids_changes = HashMap::new();
        let (xmin, xmax) = tiles
            .keys()
            .map(|(x, _)| x)
            .copied()
            .minmax()
            .into_option()
            .unwrap();
        let (ymin, ymax) = tiles
            .keys()
            .map(|(_, y)| y)
            .copied()
            .minmax()
            .into_option()
            .unwrap();
        let mut tile_id = 0;
        for y in ymin..=ymax {
            for x in xmin..=xmax {
                if let Some(tile_ways) = tiles.get(&(x, y)) {
                    let mut way_in_tile_count = 0;
                    for way_id in tile_ways {
                        let way = &ways[*way_id];
                        binary_ways.push(way.len() as u8);
                        binary_ways.extend(
                            way.iter()
                                .flat_map(|node_id| nodes[*node_id].encode(x, y, side)),
                        );
                        ids_changes.insert(way_id, (tile_id, way_in_tile_count));
                        way_in_tile_count += 1;
                    }
                }
                tiles_sizes_prefix.push(binary_ways.len());
            }
            tile_id += 1;
        }

        let new_streets = streets
            .into_iter()
            .map(|(name, street)| {
                (
                    name,
                    street
                        .iter()
                        .map(|old_id| ids_changes[old_id])
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        Map {
            binary_ways,
            start_coordinates: (xmin as f64 * side, ymin as f64 * side),
            tiles_sizes_prefix,
            tiles_per_line: xmax + 1 - xmin,
            side,
            streets: new_streets,
        }
    }

    pub fn decompress(&self) -> (Vec<Node>, Vec<Vec<NodeId>>) {
        let mut position = 0;
        let mut nodes = Vec::new();
        let mut seen_nodes = HashMap::new();
        let mut ways = Vec::new();
        for tile_number in 0..self.tiles_sizes_prefix.len() {
            let tile_x = tile_number % self.tiles_per_line;
            let tile_y = tile_number / self.tiles_per_line;
            for way_nodes in self.tile_ways(tile_x, tile_y) {
                let mut way = Vec::new();
                for node in way_nodes {
                    let node_id = *seen_nodes.entry(node).or_insert_with(|| {
                        let new_id = nodes.len();
                        nodes.push(node);
                        new_id
                    });
                    way.push(node_id);
                }
                ways.push(way);
            }
        }
        (nodes, ways)
    }
    // return map size in bytes, tiles number and max ways per tile
    pub fn stats(&self) -> (usize, usize, usize) {
        (
            self.binary_ways.len(),
            self.tiles_sizes_prefix.len(),
            std::iter::once(0)
                .chain(self.tiles_sizes_prefix.iter().copied())
                .tuple_windows()
                .map(|(a, b)| b - a)
                .max()
                .unwrap_or_default(),
        )
    }
    pub fn tile_ways(&self, tile_x: usize, tile_y: usize) -> impl Iterator<Item = Vec<Node>> + '_ {
        let tile_number = tile_y * self.tiles_per_line + tile_x;
        let tile_x = tile_number % self.tiles_per_line;
        let tile_y = tile_number / self.tiles_per_line;
        let binary_end = self.tiles_sizes_prefix[tile_number];
        let binary_start = tile_number
            .checked_sub(1)
            .map(|i| self.tiles_sizes_prefix[i])
            .unwrap_or_default();
        let mut binary_tile = &self.binary_ways[binary_start..binary_end];
        std::iter::from_fn(move || {
            if let Some((way_length, remainder)) = binary_tile.split_first() {
                let (binary_way, remainder) = remainder.split_at(2 * *way_length as usize);
                let r: &[u8] = binary_way;
                binary_tile = remainder;
                Some(
                    binary_way
                        .iter()
                        .tuples()
                        .map(|(cx, cy)| {
                            let x = self.start_coordinates.0
                                + tile_x as f64 * self.side
                                + *cx as f64 / 255. * self.side;
                            let y = self.start_coordinates.1
                                + tile_y as f64 * self.side
                                + *cy as f64 / 255. * self.side;
                            Node::new(x, y)
                        })
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
    }
}
