use itertools::Itertools;
use std::collections::HashMap;
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};

use crate::{CNodeId, CWayId, Node, NodeId, TileKey, WayId};

pub struct Map {
    pub binary_ways: Vec<u8>,
    pub start_coordinates: (f64, f64),
    pub first_tile: (usize, usize),
    pub tiles_sizes_prefix: Vec<usize>,
    pub grid_size: (usize, usize),
    pub side: f64,
    pub streets: HashMap<String, Vec<CWayId>>,
}

impl Map {
    pub fn new(
        nodes: &[Node],
        ways: &[[NodeId; 2]],
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
                    let (nodes, ways) = compress_tile(
                        nodes,
                        ways,
                        x,
                        y,
                        tile_ways,
                        side,
                        &mut ids_changes,
                        tile_id,
                    );
                    binary_ways.push(nodes.len() as u8);
                    binary_ways.extend(nodes.iter().flatten().copied());
                    binary_ways.push(ways.len() as u8);
                    binary_ways.extend(ways.iter().flatten().copied());
                }
                tiles_sizes_prefix.push(binary_ways.len());
                tile_id += 1;
            }
        }

        let new_streets: HashMap<_, _> = streets
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
            first_tile: (xmin, ymin),
            start_coordinates: (xmin as f64 * side, ymin as f64 * side),
            tiles_sizes_prefix,
            grid_size: ((xmax + 1 - xmin), (ymax + 1 - ymin)),
            side,
            streets: new_streets,
        }
    }

    pub async fn save<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        let mut writer = BufWriter::new(File::create(path).await?);
        // first, the header
        writer
            .write_all(&(self.first_tile.0 as u32).to_le_bytes())
            .await?;
        writer
            .write_all(&(self.first_tile.1 as u32).to_le_bytes())
            .await?;
        writer
            .write_all(&(self.grid_size.0 as u32).to_le_bytes())
            .await?;
        writer
            .write_all(&(self.grid_size.1 as u32).to_le_bytes())
            .await?;
        writer
            .write_all(&self.start_coordinates.0.to_le_bytes())
            .await?;
        writer
            .write_all(&self.start_coordinates.1.to_le_bytes())
            .await?;
        writer.write_all(&self.side.to_le_bytes()).await?;

        // now the tiles sizes, encoded on 24 bytes
        for s in &self.tiles_sizes_prefix {
            assert!(*s <= 1 << 24);
            writer.write_all(&(*s as u32).to_le_bytes()[0..3]).await?;
        }

        // now, all tiled ways ; size is last element of sizes_prefix
        writer.write_all(&self.binary_ways).await?;

        Ok(())
    }

    pub fn node_tiles(&self, node: &Node) -> impl Iterator<Item = (usize, usize)> + '_ {
        node.tiles(self.side).map(|(x, y)| {
            (
                x.checked_sub(self.first_tile.0).unwrap(),
                y.checked_sub(self.first_tile.1).unwrap(),
            )
        })
    }

    pub fn ways(&self) -> impl Iterator<Item = [Node; 2]> + '_ {
        (0..self.tiles_sizes_prefix.len())
            .flat_map(|tile_number| self.tile_ways(tile_number as u16).map(|(_, n)| n))
    }

    pub fn decompress(&self) -> (Vec<Node>, Vec<Vec<NodeId>>) {
        let mut nodes = Vec::new();
        let mut seen_nodes = HashMap::new();
        let mut ways = Vec::new();
        for way_nodes in self.ways() {
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
        (nodes, ways)
    }
    // return map size in bytes, tiles number and max ways per tile
    pub fn stats(&self) -> (usize, usize, usize) {
        // let max_tile = (0..self.tiles_sizes_prefix.len())
        //     .max_by_key(|tile_number| {
        //         let tile_x = tile_number % self.grid_size.0;
        //         let tile_y = tile_number / self.grid_size.0;
        //         self.tile_ways(tile_x, tile_y).count()
        //     })
        //     .unwrap();
        // let xmin = self.start_coordinates.0 + self.side * (max_tile % self.grid_size.0) as f64;
        // let ymin = self.start_coordinates.1 + self.side * (max_tile / self.grid_size.0) as f64;
        // let xmax = xmin + self.side;
        // let ymax = ymin + self.side;
        // eprintln!("max tile is located at {xmin},{ymin},{xmax},{ymax}");

        (
            self.binary_ways.len(),
            self.tiles_sizes_prefix.len(),
            (0..self.tiles_sizes_prefix.len())
                .map(|tile_number| self.tile_ways(tile_number as u16).count())
                .max()
                .unwrap(),
        )
    }

    pub fn tile_binary(&self, tile_number: u16) -> &[u8] {
        let binary_end = self.tiles_sizes_prefix[tile_number as usize];
        let binary_start = tile_number
            .checked_sub(1)
            .map(|i| self.tiles_sizes_prefix[i as usize])
            .unwrap_or_default();
        &self.binary_ways[binary_start..binary_end]
    }

    // get number of nodes inside given tile
    pub fn tile_nodes_number(&self, tile_number: u16) -> u8 {
        self.tile_binary(tile_number)
            .first()
            .copied()
            .unwrap_or_default()
    }

    // get number of ways inside given tile
    pub fn tile_ways_number(&self, tile_number: u16) -> u8 {
        let binary_tile = self.tile_binary(tile_number);
        binary_tile
            .first()
            .and_then(|&nodes_number| binary_tile.get(1 + 2 * nodes_number as usize))
            .copied()
            .unwrap_or_default()
    }

    // loop on all ways inside given tile
    pub fn tile_ways(&self, tile_number: u16) -> impl Iterator<Item = (CWayId, [Node; 2])> + '_ {
        (0..(self.tile_ways_number(tile_number))).map(move |local_way_id| {
            let way_id = CWayId {
                tile_number,
                local_way_id,
            };
            (way_id, self.decode_way(way_id))
        })
    }

    pub(crate) fn decode_node(&self, node_id: CNodeId) -> Node {
        let tile_x = node_id.tile_number as usize % self.grid_size.0;
        let tile_y = node_id.tile_number as usize / self.grid_size.0;
        let binary_tile = self.tile_binary(node_id.tile_number);
        let cx = binary_tile[2 * node_id.local_node_id as usize + 1];
        let cy = binary_tile[2 * node_id.local_node_id as usize + 2];

        let x = self.start_coordinates.0 + tile_x as f64 * self.side + cx as f64 / 255. * self.side;
        let y = self.start_coordinates.1 + tile_y as f64 * self.side + cy as f64 / 255. * self.side;
        Node::new(x, y)
    }

    fn decode_way(&self, way_id: CWayId) -> [Node; 2] {
        let nodes_number = self.tile_nodes_number(way_id.tile_number);
        let binary_tile = self.tile_binary(way_id.tile_number);
        let ways_binary = &binary_tile[(2 + 2 * nodes_number as usize)..];
        let n1 = ways_binary[2 * way_id.local_way_id as usize];
        let n2 = ways_binary[2 * way_id.local_way_id as usize + 1];
        [
            self.decode_node(CNodeId {
                tile_number: way_id.tile_number,
                local_node_id: n1,
            }),
            self.decode_node(CNodeId {
                tile_number: way_id.tile_number,
                local_node_id: n2,
            }),
        ]
    }

    pub fn bounding_box(&self) -> (f64, f64, f64, f64) {
        let (xmin, ymin) = self.start_coordinates;
        let xmax = xmin + self.grid_size.0 as f64 * self.side;
        let ymax = ymin + self.grid_size.1 as f64 * self.side;
        (xmin, ymin, xmax, ymax)
    }
}

fn compress_tile(
    nodes: &[Node],
    ways: &[[NodeId; 2]],
    tile_x: usize,
    tile_y: usize,
    tile_ways: &[WayId],
    side: f64,
    ids_changes: &mut HashMap<WayId, CWayId>,
    tile_id: usize,
) -> (Vec<[u8; 2]>, Vec<[u8; 2]>) {
    let mut compressed_nodes = Vec::new();
    let mut seen_compressed_nodes: HashMap<[u8; 2], u8> = HashMap::new();
    let mut compressed_ways = Vec::new();

    for (local_way_id, global_way_id) in tile_ways.iter().enumerate() {
        let mut new_way = Vec::new();
        for node in ways[*global_way_id].iter().map(|&i| &nodes[i]) {
            let new_node = node.encode(tile_x, tile_y, side);
            let new_node_id = *seen_compressed_nodes.entry(new_node).or_insert_with(|| {
                let new_node_id = compressed_nodes.len();
                assert!(new_node_id <= 255);
                compressed_nodes.push(new_node);
                new_node_id as u8
            });
            new_way.push(new_node_id);
        }
        ids_changes.insert(
            *global_way_id,
            CWayId {
                tile_number: tile_id as u16,
                local_way_id: local_way_id as u8,
            },
        );
        compressed_ways.push([new_way[0], new_way[1]]);
    }

    (compressed_nodes, compressed_ways)
}
