use itertools::Itertools;
use std::collections::HashMap;

use crate::{Node, NodeId, TileKey, WayId};

pub struct CompressedMap {
    binary_ways: Vec<u8>,
    start_coordinates: (f64, f64),
    first_way_in_each_tile: Vec<usize>,
    tiles_per_line: usize,
    side: f64,
}

impl CompressedMap {
    pub fn new(
        nodes: &[Node],
        ways: &[Vec<NodeId>],
        streets: &mut HashMap<String, Vec<WayId>>,
        tiles: &HashMap<TileKey, Vec<WayId>>,
        side: f64,
    ) -> Self {
        let mut binary_ways = Vec::new();
        let mut tiles_sizes_prefix = Vec::new();
        let mut processed_ways_count = 0;
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
        for y in ymin..=ymax {
            for x in xmin..=xmax {
                tiles_sizes_prefix.push(processed_ways_count);
                if let Some(tile_ways) = tiles.get(&(x, y)) {
                    for way_id in tile_ways {
                        let way = &ways[*way_id];
                        binary_ways.push(way.len() as u8);
                        binary_ways.extend(
                            way.iter()
                                .flat_map(|node_id| nodes[*node_id].encode(x, y, side)),
                        );
                        ids_changes.insert(way_id, processed_ways_count);
                        processed_ways_count += 1;
                    }
                }
            }
        }
        tiles_sizes_prefix.push(processed_ways_count);

        for street in streets.values_mut() {
            let new_street = street
                .iter()
                .map(|old_id| ids_changes[old_id])
                .collect::<Vec<_>>();
            *street = new_street;
        }
        CompressedMap {
            binary_ways,
            start_coordinates: (xmin as f64 * side, ymin as f64 * side),
            first_way_in_each_tile: tiles_sizes_prefix,
            tiles_per_line: xmax + 1 - xmin,
            side,
        }
    }

    pub fn decompress(&self) -> (Vec<Node>, Vec<Vec<NodeId>>) {
        let mut position = 0;
        let mut nodes = Vec::new();
        let mut seen_nodes = HashMap::new();
        let mut ways = Vec::new();
        while position < self.binary_ways.len() {
            let way_length = self.binary_ways[position];
            position += 1;
            let tile_number = match self.first_way_in_each_tile.binary_search(&ways.len()) {
                Ok(i) => {
                    // we need the rightmost match
                    self.first_way_in_each_tile[i..]
                        .iter()
                        .enumerate()
                        .take_while(|&(_, s)| *s == ways.len())
                        .last()
                        .map(|(i, _)| i)
                        .unwrap()
                        + i
                }
                Err(i) => i - 1,
            };
            let tile_x = tile_number % self.tiles_per_line;
            let tile_y = tile_number / self.tiles_per_line;
            let mut way = Vec::new();
            for i in 0..way_length {
                let cx = self.binary_ways[position + 2 * i as usize];
                let cy = self.binary_ways[position + 2 * i as usize + 1];
                let x = self.start_coordinates.0
                    + tile_x as f64 * self.side
                    + cx as f64 / 255. * self.side;
                let y = self.start_coordinates.1
                    + tile_y as f64 * self.side
                    + cy as f64 / 255. * self.side;
                let node = Node::new(x, y);
                let node_id = *seen_nodes.entry(node).or_insert_with(|| {
                    let new_id = nodes.len();
                    nodes.push(node);
                    new_id
                });
                way.push(node_id);
            }
            position += 2 * way_length as usize;
            ways.push(way);
        }
        (nodes, ways)
    }
}
