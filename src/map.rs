use itertools::Itertools;
use std::collections::HashMap;
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};

use crate::{CWayId, Node, NodeId, TileKey, WayId};

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
                    let tile_start_offset = binary_ways.len();
                    for way_id in tile_ways {
                        let way = &ways[*way_id];
                        let way_offset = binary_ways.len() - tile_start_offset;
                        binary_ways.push(way.len() as u8);
                        binary_ways.extend(
                            way.iter()
                                .flat_map(|node_id| nodes[*node_id].encode(x, y, side)),
                        );
                        ids_changes.insert(way_id, (tile_id, way_offset as u16));
                    }
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
        writer.write_all(&self.first_tile.0.to_le_bytes()).await?;
        writer.write_all(&self.first_tile.1.to_le_bytes()).await?;
        writer.write_all(&self.grid_size.0.to_le_bytes()).await?;
        writer.write_all(&self.grid_size.1.to_le_bytes()).await?;
        writer
            .write_all(&self.start_coordinates.0.to_le_bytes())
            .await?;
        writer
            .write_all(&self.start_coordinates.1.to_le_bytes())
            .await?;
        writer.write_all(&self.side.to_le_bytes()).await?;
        // now, all tiled ways
        writer
            .write_all(&self.binary_ways.len().to_le_bytes())
            .await?;
        writer.write_all(&self.binary_ways).await?;
        // now the tiles sizes
        writer
            .write_all(&self.tiles_sizes_prefix.len().to_le_bytes())
            .await?;
        for s in &self.tiles_sizes_prefix {
            assert!(*s <= std::u32::MAX as usize); // for now
            writer.write_all(&(*s as u32).to_le_bytes()).await?;
        }
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

    pub fn ways(&self) -> impl Iterator<Item = Vec<Node>> + '_ {
        (0..self.tiles_sizes_prefix.len()).flat_map(|tile_number| {
            let tile_x = tile_number % self.grid_size.0;
            let tile_y = tile_number / self.grid_size.0;
            self.tile_ways(tile_x, tile_y).map(|(_, n)| n)
        })
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
                .map(|tile_number| {
                    let tile_x = tile_number % self.grid_size.0;
                    let tile_y = tile_number / self.grid_size.0;
                    self.tile_ways(tile_x, tile_y).count()
                })
                .max()
                .unwrap(),
        )
    }

    pub fn tile_ways(
        &self,
        tile_x: usize,
        tile_y: usize,
    ) -> impl Iterator<Item = (usize, Vec<Node>)> + '_ {
        let tile_number = tile_y * self.grid_size.0 + tile_x;
        let binary_end = self.tiles_sizes_prefix[tile_number];
        let binary_start = tile_number
            .checked_sub(1)
            .map(|i| self.tiles_sizes_prefix[i])
            .unwrap_or_default();
        let mut binary_tile = &self.binary_ways[binary_start..binary_end];
        std::iter::from_fn(move || {
            self.decode_way(
                tile_x,
                tile_y,
                binary_tile,
                self.binary_ways.as_ptr() as usize,
            )
            .map(|(way_offset, way, remainder)| {
                binary_tile = remainder;
                (way_offset, way)
            })
        })
    }

    // loop on both endpoints of all ways in wanted tile
    // we also get a usize which is the way's starting offset
    // in the binary encoding and therefore a unique and 'compact'
    // identifier of the way
    pub fn tile_ways_ends(
        &self,
        tile_x: usize,
        tile_y: usize,
    ) -> impl Iterator<Item = (usize, [Node; 2])> + '_ {
        //TODO: optimize me
        self.tile_ways(tile_x, tile_y).map(|(way_offset, nodes)| {
            (
                way_offset,
                [
                    nodes.first().copied().unwrap(),
                    nodes.last().copied().unwrap(),
                ],
            )
        })
    }

    pub fn way(&self, way_id: CWayId) -> (usize, Vec<Node>) {
        let (tile_number, way_offset) = (way_id.0 as usize, way_id.1 as usize);
        let tile_x = tile_number % self.grid_size.0;
        let tile_y = tile_number / self.grid_size.0;
        let tile_start = tile_number
            .checked_sub(1)
            .map(|i| self.tiles_sizes_prefix[i])
            .unwrap_or_default();
        let offset = tile_start + way_offset;
        let binary = &self.binary_ways[offset..];
        let decoded = self
            .decode_way(
                tile_x,
                tile_y,
                binary,
                &self.binary_ways[0] as *const _ as usize,
            )
            .unwrap();
        (decoded.0, decoded.1)
    }

    pub fn way_length(&self, way_offset: usize) -> f64 {
        let way_size = self.binary_ways[way_offset] as usize;
        let binary_way = &self.binary_ways[way_offset + 1..(way_offset + 1 + 2 * way_size)];
        binary_way
            .iter()
            .map(|b| self.side * *b as f64 / 255.)
            .tuples()
            .map(|(x, y)| Node::new(x, y))
            .tuple_windows()
            .map(|(n1, n2)| n1.squared_distance_between(&n2).sqrt())
            .sum::<f64>()
    }

    fn decode_way<'a>(
        &self,
        tile_x: usize,
        tile_y: usize,
        binary_tile: &'a [u8],
        binary_start: usize,
    ) -> Option<(usize, Vec<Node>, &'a [u8])> {
        let way_offset = binary_tile.as_ptr() as usize - binary_start;
        binary_tile.split_first().map(|(way_length, remainder)| {
            let (binary_way, remainder) = remainder.split_at(2 * *way_length as usize);
            (
                way_offset,
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
                remainder,
            )
        })
    }

    pub fn bounding_box(&self) -> (f64, f64, f64, f64) {
        let (xmin, ymin) = self.start_coordinates;
        let xmax = xmin + self.grid_size.0 as f64 * self.side;
        let ymax = ymin + self.grid_size.1 as f64 * self.side;
        (xmin, ymin, xmax, ymax)
    }
}
