use itertools::Itertools;
use std::{collections::HashMap, io::Read, path::Path};
use tokio::io::AsyncWriteExt;

pub const SIDE: f64 = 1. / 1000.; // excellent value
                                  // with it we have few segments crossing several squares
                                  // and what's more we can use 1 byte for each coordinate inside the square
                                  // for 1/2 meter precision

use crate::{CNodeId, CWayId, Node, NodeId, TileKey, WayId};

pub(crate) enum BlockType {
    Tiles,
    Streets,
    Path,
    Interests,
}

pub struct Map {
    pub binary_ways: Vec<u8>,
    pub start_coordinates: (f64, f64),
    pub first_tile: (usize, usize),
    pub tiles_sizes_prefix: Vec<usize>,
    pub grid_size: (usize, usize),
    pub side: f64,
    pub streets: HashMap<String, Vec<CWayId>>,
    pub interests: Vec<(usize, Node)>,
}

impl From<Vec<Node>> for Map {
    fn from(mut nodes: Vec<Node>) -> Self {
        let mut ways = vec![(0..nodes.len()).collect::<Vec<_>>()];
        let mut streets = HashMap::new();
        crate::cut_segments_on_tiles(&mut nodes, &mut ways, SIDE);
        let ways = crate::cut_ways_into_edges(ways, &mut streets);
        let tiles = crate::group_ways_in_tiles(&nodes, &ways, SIDE);
        Map::new(&nodes, &ways, streets, &tiles, Vec::new(), SIDE)
    }
}

impl Map {
    pub fn load<P: AsRef<Path>>(path: P, key_values: &[(String, String)]) -> std::io::Result<Self> {
        let mut answer = Vec::new();
        std::io::BufReader::new(std::fs::File::open(path.as_ref())?).read_to_end(&mut answer)?;
        let string = std::str::from_utf8(&answer).unwrap();
        Ok(Map::from_string(string, key_values))
    }
    pub fn from_string(s: &str, key_values: &[(String, String)]) -> Self {
        let (nodes, mut ways, mut streets, mut interests) = crate::parse_osm_xml(s, key_values);
        interests.sort_unstable_by(|(_, n1), (_, n2)| n1.x.partial_cmp(&n2.x).unwrap());
        let mut renamed_nodes = crate::rename_nodes(nodes, &mut ways);
        let mut ways = crate::sanitize_ways(ways, &mut streets);
        crate::simplify_ways(&mut renamed_nodes, &mut ways, &mut streets);
        crate::cut_segments_on_tiles(&mut renamed_nodes, &mut ways, SIDE);
        let ways = crate::cut_ways_into_edges(ways, &mut streets);
        let tiles = crate::group_ways_in_tiles(&renamed_nodes, &ways, SIDE);
        Map::new(&renamed_nodes, &ways, streets, &tiles, interests, SIDE)
    }
    pub fn new(
        nodes: &[Node],
        ways: &[[NodeId; 2]],
        streets: HashMap<String, Vec<WayId>>,
        tiles: &HashMap<TileKey, Vec<WayId>>,
        interests: Vec<(usize, Node)>,
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
                    let ways = compress_tile(
                        nodes,
                        ways,
                        x,
                        y,
                        tile_ways,
                        side,
                        &mut ids_changes,
                        tile_id,
                    );
                    binary_ways.extend(ways.iter().flatten().flatten().copied());
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
            interests,
        }
    }

    pub async fn save_tiles<W: AsyncWriteExt + std::marker::Unpin>(
        &self,
        writer: &mut W,
        color: &[u8; 3],
    ) -> std::io::Result<()> {
        writer.write_u8(BlockType::Tiles as u8).await?;
        writer.write_all(color).await?;

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

        self.save_sizes_prefix(writer).await?;
        // for s in &self.tiles_sizes_prefix {
        //     assert!(*s <= 1 << 24);
        //     writer.write_all(&(*s as u32).to_le_bytes()[0..3]).await?;
        // }

        // now, all tiled ways ; size is last element of sizes_prefix
        writer.write_all(&self.binary_ways).await?;
        Ok(())
    }

    pub async fn save_interests<W: AsyncWriteExt + std::marker::Unpin>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        writer.write_u8(BlockType::Interests as u8).await?;
        writer.write_u16(self.interests.len() as u16).await?;
        for (interest, _) in &self.interests {
            writer.write_u8(*interest as u8).await?;
        }
        for (_, node) in &self.interests {
            writer.write_f64(node.x).await?;
            writer.write_f64(node.y).await?;
        }
        Ok(())
    }

    pub async fn save_sizes_prefix<W: AsyncWriteExt + std::marker::Unpin>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let non_empty_tiles = std::iter::once([0, self.tiles_sizes_prefix[0]].as_slice())
            .chain(self.tiles_sizes_prefix.windows(2))
            .enumerate()
            .filter_map(|(i, w)| if w[0] != w[1] { Some(i as u16) } else { None })
            .collect::<Vec<u16>>();
        writer
            .write_all(&(non_empty_tiles.len() as u16).to_le_bytes())
            .await?;
        for tile in &non_empty_tiles {
            writer.write_all(&tile.to_le_bytes()).await?;
        }
        for end in non_empty_tiles
            .iter()
            .map(|tile_index| self.tiles_sizes_prefix[*tile_index as usize])
        {
            writer.write_all(&end.to_le_bytes()[0..3]).await?;
        }
        Ok(())
    }

    pub async fn save_streets<W: AsyncWriteExt + std::marker::Unpin>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        writer.write_u8(BlockType::Streets as u8).await?;

        // finally, write all streets data
        let encoded = crate::streets::encode_streets(&self.streets);
        // let streets_back = crate::streets::decode_streets(&encoded);
        // self.streets.iter().for_each(|(name, ways)| {
        //     let restored_ways = streets_back.get(name).unwrap();
        //     ways.iter().zip(restored_ways).all(|(w1, w2)| w1 == w2);
        // });
        writer.write_all(&encoded).await?;

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

    // get number of ways inside given tile
    pub fn tile_ways_number(&self, tile_number: u16) -> u8 {
        (self.tile_binary(tile_number).len() / 4) as u8
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
        let cx = binary_tile[2 * node_id.local_node_id as usize];
        let cy = binary_tile[2 * node_id.local_node_id as usize + 1];

        let x = self.start_coordinates.0 + tile_x as f64 * self.side + cx as f64 / 255. * self.side;
        let y = self.start_coordinates.1 + tile_y as f64 * self.side + cy as f64 / 255. * self.side;
        Node::new(x, y)
    }

    fn decode_way(&self, way_id: CWayId) -> [Node; 2] {
        [
            self.decode_node(CNodeId {
                tile_number: way_id.tile_number,
                local_node_id: 2 * way_id.local_way_id,
            }),
            self.decode_node(CNodeId {
                tile_number: way_id.tile_number,
                local_node_id: 2 * way_id.local_way_id + 1,
            }),
        ]
    }

    pub fn bounding_box(&self) -> (f64, f64, f64, f64) {
        let (xmin, ymin) = self.start_coordinates;
        let xmax = xmin + self.grid_size.0 as f64 * self.side;
        let ymax = ymin + self.grid_size.1 as f64 * self.side;
        (xmin, ymin, xmax, ymax)
    }

    pub(crate) fn node_offset_id(&self, id: &CNodeId) -> usize {
        let tile_offset = id
            .tile_number
            .checked_sub(1)
            .map(|i| self.tiles_sizes_prefix[i as usize])
            .unwrap_or_default();
        let offset = tile_offset + 2 * id.local_node_id as usize;
        assert!(offset % 2 == 0);
        offset / 2
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
) -> Vec<[[u8; 2]; 2]> {
    let mut compressed_ways = Vec::new();

    for (local_way_id, global_way_id) in tile_ways.iter().enumerate() {
        let mut new_way = Vec::new();
        for node in ways[*global_way_id].iter().map(|&i| &nodes[i]) {
            let new_node = node.encode(tile_x, tile_y, side);
            new_way.push(new_node);
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

    compressed_ways
}
