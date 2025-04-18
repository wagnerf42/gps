use futures::io::AsyncWriteExt;
use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::Path,
};

// pub const SIDE: f64 = 1. / 150.; // ski value
pub const DEFAULT_SIDE: f64 = 1. / 750.; // excellent value
                                         // with it we have few segments crossing several squares
                                         // and what's more we can use 1 byte for each coordinate inside the square
                                         // for 1 meter precision
                                         // Note that the best value for size is 1/500
                                         // But we go for 1/750 because this enables is to use less pixels in the watch's display

use crate::{CNodeId, CWayId, Node, NodeId, TileKey, WayId};

pub enum BlockType {
    Tiles,
    Streets,
    Path,
    Interests,
    Heights,
}

pub struct Map {
    pub color: [u8; 3],
    pub binary_ways: Vec<u8>,
    pub start_coordinates: (f64, f64),
    pub first_tile: (isize, isize),
    pub tiles_sizes_prefix: Vec<usize>,
    pub grid_size: (usize, usize),
    pub side: f64,
    pub streets: HashMap<String, Vec<CWayId>>,
}

pub fn load_maps_and_interests<P: AsRef<Path>>(
    path: P,
    key_values: &[(String, String)],
    ski: bool,
) -> std::io::Result<(Vec<Map>, Vec<(usize, Node)>)> {
    let mut answer = Vec::new();
    std::io::BufReader::new(std::fs::File::open(path.as_ref())?).read_to_end(&mut answer)?;
    let string = std::str::from_utf8(&answer).unwrap();
    let side = if ski {
        1. / 150.
    } else {
        crate::map::DEFAULT_SIDE
    };
    Ok(maps_and_interests_from_string(
        string, key_values, ski, side,
    ))
}

pub fn maps_and_interests_from_string(
    s: &str,
    key_values: &[(String, String)],
    ski: bool,
    side: f64,
) -> (Vec<Map>, Vec<(usize, Node)>) {
    crate::log("map: parsing xml");
    let (nodes, mut ways, mut streets, pistes, interests) = crate::parse_osm_xml(s, key_values);
    if ski {
        // red is 254 because at 255 gipy would display it thick
        let colors = [
            [0, 255, 0],
            [0, 0, 255],
            [254, 0, 0],
            [0, 0, 0],
            [255, 0, 255],
        ];
        let mut maps = Vec::new();
        for (color, pistes) in colors.into_iter().zip(&pistes) {
            let nodes = nodes.clone();
            let mut ways = ways
                .iter()
                .filter(|&(id, _)| pistes.contains(id))
                .map(|(id, nodes)| (*id, nodes.clone()))
                .collect::<HashMap<_, _>>();
            if ways.is_empty() {
                continue;
            }
            let mut streets = HashMap::new();
            let mut renamed_nodes = crate::rename_nodes(nodes, &mut ways);
            let mut ways = crate::sanitize_ways(ways, &mut streets);
            crate::simplify_ways(&mut renamed_nodes, &mut ways, &mut streets);
            crate::cut_segments_on_tiles(&mut renamed_nodes, &mut ways, side);
            let ways = crate::cut_ways_into_edges(ways, &mut streets);
            let tiles = crate::group_ways_in_tiles(&renamed_nodes, &ways, side);
            let map = Map::new(color, &renamed_nodes, &ways, streets, &tiles, side);
            maps.push(map);
        }
        if maps.is_empty() {
            crate::log("map: no ski pistes found");
        }
        (maps, interests)
    } else {
        crate::log("map: building");
        let mut renamed_nodes = crate::rename_nodes(nodes, &mut ways);
        let mut ways = crate::sanitize_ways(ways, &mut streets);
        crate::simplify_ways(&mut renamed_nodes, &mut ways, &mut streets);
        crate::cut_segments_on_tiles(&mut renamed_nodes, &mut ways, side);
        let ways = crate::cut_ways_into_edges(ways, &mut streets);
        let tiles = crate::group_ways_in_tiles(&renamed_nodes, &ways, side);
        crate::log("map: done");
        (
            vec![Map::new(
                [0, 0, 0],
                &renamed_nodes,
                &ways,
                streets,
                &tiles,
                side,
            )],
            interests,
        )
    }
}

impl Map {
    pub fn new(
        color: [u8; 3],
        nodes: &[Node],
        ways: &[[NodeId; 2]],
        streets: HashMap<String, Vec<WayId>>,
        tiles: &HashMap<TileKey, Vec<WayId>>,
        side: f64,
    ) -> Self {
        let mut binary_ways = Vec::new();
        let mut tiles_sizes_prefix = Vec::new();
        let mut ids_changes = HashMap::new();
        let mut local_ids_changes: HashMap<CWayId, CWayId> = HashMap::new();
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
                    let mut ways = compress_tile(
                        nodes,
                        ways,
                        x,
                        y,
                        tile_ways,
                        side,
                        &mut ids_changes,
                        tile_id,
                    );
                    deduplicate_ways(&mut ways, tile_id, &mut local_ids_changes);
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
                        .filter_map(|old_id| ids_changes.get(old_id).copied())
                        .map(|old_local_id| {
                            local_ids_changes
                                .get(&old_local_id)
                                .copied()
                                .unwrap_or(old_local_id)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        Map {
            color,
            binary_ways,
            first_tile: (xmin, ymin),
            start_coordinates: (xmin as f64 * side, ymin as f64 * side),
            tiles_sizes_prefix,
            grid_size: ((xmax + 1 - xmin) as usize, (ymax + 1 - ymin) as usize),
            side,
            streets: new_streets,
        }
    }

    pub fn from_path(mut nodes: Vec<Node>, side: f64) -> Self {
        let mut ways = vec![(0..nodes.len() as u64).collect::<Vec<_>>()];
        let mut streets = HashMap::new();
        crate::cut_segments_on_tiles(&mut nodes, &mut ways, side);
        let ways = crate::cut_ways_into_edges(ways, &mut streets);
        let tiles = crate::group_ways_in_tiles(&nodes, &ways, side);
        Map::new([255, 0, 0], &nodes, &ways, streets, &tiles, side)
    }

    pub fn non_empty_tiles(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.grid_size.0).flat_map(|tile_x| {
            (0..self.grid_size.1)
                .map(move |tile_y| (tile_x, tile_y))
                .filter(|(tile_x, tile_y)| {
                    self.tile_ways_number((tile_x + tile_y * self.grid_size.0) as u16) > 0
                })
        })
    }

    pub fn save_tiles<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&[BlockType::Tiles as u8])?;
        writer.write_all(&self.color)?;

        // first, the header
        writer.write_all(&(self.first_tile.0 as i32).to_le_bytes())?;
        writer.write_all(&(self.first_tile.1 as i32).to_le_bytes())?;
        writer.write_all(&(self.grid_size.0 as u32).to_le_bytes())?;
        writer.write_all(&(self.grid_size.1 as u32).to_le_bytes())?;
        writer.write_all(&self.start_coordinates.0.to_le_bytes())?;
        writer.write_all(&self.start_coordinates.1.to_le_bytes())?;
        writer.write_all(&self.side.to_le_bytes())?;

        self.save_sizes_prefix(writer)?;
        // for s in &self.tiles_sizes_prefix {
        //     assert!(*s <= 1 << 24);
        //     writer.write_all(&(*s as u32).to_le_bytes()[0..3]).await?;
        // }

        // now, all tiled ways ; size is last element of sizes_prefix
        writer.write_all(&self.binary_ways)?;
        Ok(())
    }

    pub fn save_sizes_prefix<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let non_empty_tiles = std::iter::once([0, self.tiles_sizes_prefix[0]].as_slice())
            .chain(self.tiles_sizes_prefix.windows(2))
            .enumerate()
            .filter_map(|(i, w)| if w[0] != w[1] { Some(i) } else { None })
            .collect::<Vec<usize>>();
        let bytes_number = if self.tiles_sizes_prefix.last().copied().unwrap_or_default() / 4
            <= std::u16::MAX as usize
        {
            writer.write_all(&[16])?;
            2
        } else {
            writer.write_all(&[24])?;
            3
        };
        writer.write_all(&[4])?; // size taken by each way
        writer.write_all(&(non_empty_tiles.len() as u16).to_le_bytes())?;

        let bytes_per_tile_index = if self.grid_size.0 * self.grid_size.1 > std::u16::MAX as usize {
            3
        } else {
            2
        };
        for tile in &non_empty_tiles {
            writer.write_all(&tile.to_le_bytes()[0..bytes_per_tile_index])?;
        }
        for end in non_empty_tiles
            .iter()
            .map(|tile_index| self.tiles_sizes_prefix[*tile_index as usize])
            // compute position in ways not in bytes
            .map(|end| {
                assert_eq!(end % 4, 0);
                end / 4
            })
        {
            writer.write_all(&end.to_le_bytes()[0..bytes_number])?;
        }
        Ok(())
    }

    pub async fn save_streets<W: AsyncWriteExt + std::marker::Unpin>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        writer.write_all(&[BlockType::Streets as u8]).await?;

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
        node.tiles(self.side).filter_map(|(x, y)| {
            if x >= self.first_tile.0
                && y >= self.first_tile.1
                && x - self.first_tile.0 < self.grid_size.0 as isize
                && y - self.first_tile.1 < self.grid_size.1 as isize
            {
                Some((
                    (x - self.first_tile.0) as usize,
                    (y - self.first_tile.1) as usize,
                ))
            } else {
                None
            }
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
                    let new_id = nodes.len() as u64;
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
                local_node_id: 2 * way_id.local_way_id as u16,
            }),
            self.decode_node(CNodeId {
                tile_number: way_id.tile_number,
                local_node_id: 2 * way_id.local_way_id as u16 + 1,
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

    // discard empty tiles on border
    pub fn fit_map(&mut self) {
        let (xmin, xmax) = self
            .non_empty_tiles()
            .map(|(x, _)| x)
            .minmax()
            .into_option()
            .unwrap();
        let (ymin, ymax) = self
            .non_empty_tiles()
            .map(|(_, y)| y)
            .minmax()
            .into_option()
            .unwrap();

        let mut new_prefix = Vec::new();
        let mut old_prefix = self.tiles_sizes_prefix.iter();
        for y in 0..self.grid_size.1 {
            for x in 0..self.grid_size.0 {
                let tile_end = *old_prefix.next().unwrap();
                if x >= xmin && x <= xmax && y >= ymin && y <= ymax {
                    new_prefix.push(tile_end);
                }
            }
        }

        self.tiles_sizes_prefix = new_prefix;
        self.grid_size = (xmax + 1 - xmin, ymax + 1 - ymin);
        self.first_tile = (
            self.first_tile.0 + xmin as isize,
            self.first_tile.1 + ymin as isize,
        );
        self.start_coordinates = (
            self.first_tile.0 as f64 * self.side,
            self.first_tile.1 as f64 * self.side,
        );
    }

    // discard all tiles which are not the ones we want.
    pub fn keep_tiles(&mut self, kept_tiles: &HashSet<(usize, usize)>) {
        let mut new_binary_ways: Vec<u8> = Vec::new();
        let mut new_tiles_sizes_prefix = Vec::new();
        let mut kept_ways = HashSet::new();
        let mut current_end = 0;
        for (tile_number, (tile_start, tile_end)) in std::iter::once(0)
            .chain(self.tiles_sizes_prefix.iter().copied())
            .tuple_windows()
            .enumerate()
        {
            let tile_x = tile_number % self.grid_size.0;
            let tile_y = tile_number / self.grid_size.0;
            if kept_tiles.contains(&(tile_x, tile_y)) {
                new_binary_ways.extend(&self.binary_ways[tile_start..tile_end]);
                kept_ways.extend((0..(tile_end - tile_start) / 2).map(|local_way_id| CWayId {
                    tile_number: tile_number as u16,
                    local_way_id: local_way_id as u8,
                }));
                current_end += tile_end - tile_start;
            }
            new_tiles_sizes_prefix.push(current_end);
        }
        self.binary_ways = new_binary_ways;
        self.tiles_sizes_prefix = new_tiles_sizes_prefix;
        // now filter streets
        self.streets.retain(|_, street_ways| {
            street_ways.retain(|way_id| kept_ways.contains(way_id));
            !street_ways.is_empty()
        });
    }
}

fn deduplicate_ways(
    ways: &mut Vec<[[u8; 2]; 2]>,
    tile_id: usize,
    local_ids_changes: &mut HashMap<CWayId, CWayId>,
) {
    use rational::Rational;
    let mut lines: HashMap<_, [Vec<_>; 2]> = HashMap::new();
    for (local_way_num, [start, end]) in ways.iter().copied().enumerate() {
        let [x1, y1] = start;
        let [x2, y2] = end;
        // assert!(x1 != x2 || y1 != y2);
        let key = if x1 == x2 {
            (Rational::new(256, 1), Rational::new(x1, 1))
        } else {
            let slope = Rational::new(y2 - y1, x2 - x1);
            let height = y1 - slope * x1;
            (slope, height)
        };
        let points = lines.entry(key).or_default();
        if start < end {
            points[0].push((start, local_way_num));
            points[1].push((end, local_way_num));
        } else {
            points[1].push((start, local_way_num));
            points[0].push((end, local_way_num));
        }
    }
    let mut remaining_ways = Vec::new();
    for [starts, ends] in lines.into_values() {
        let mut events: HashMap<_, [Vec<usize>; 2]> = HashMap::new();
        for (start, way_num) in starts {
            events.entry(start).or_default()[0].push(way_num);
        }
        for (end, way_num) in ends {
            events.entry(end).or_default()[1].push(way_num);
        }
        let mut current_start = None;
        let mut inner_ways = Vec::new();
        let mut current_ways = HashSet::new();
        for (point, [starts, ends]) in events.into_iter().sorted() {
            let current_count = current_ways.len();
            for end in ends {
                current_ways.remove(&end);
            }
            for start in &starts {
                current_ways.insert(*start);
            }
            let new_count = current_ways.len();
            if current_count == 0 && new_count > 0 {
                // new way start
                current_start = Some(point);
            } else if current_count > 0 && new_count == 0 {
                // new way end
                let new_way_id = CWayId {
                    tile_number: tile_id as u16,
                    local_way_id: remaining_ways.len() as u8,
                };
                remaining_ways.push([current_start.unwrap(), point]);
                for way_num in inner_ways.drain(..) {
                    let old_way_id = CWayId {
                        tile_number: tile_id as u16,
                        local_way_id: way_num as u8,
                    };
                    local_ids_changes.insert(old_way_id, new_way_id);
                }
            }
            for start in starts {
                inner_ways.push(start);
            }
        }
    }
    std::mem::swap(ways, &mut remaining_ways);
}

fn compress_tile(
    nodes: &[Node],
    ways: &[[NodeId; 2]],
    tile_x: isize,
    tile_y: isize,
    tile_ways: &[WayId],
    side: f64,
    ids_changes: &mut HashMap<WayId, CWayId>,
    tile_id: usize,
) -> Vec<[[u8; 2]; 2]> {
    let mut compressed_ways = Vec::new();

    // WIP: if we enable footways tile 1754 is huge in poisat2.gpx
    // eprintln!(
    //     "compressing tile {tile_x}/{tile_y} ({} ways)",
    //     tile_ways.len()
    // );
    // if tile_x == 2884 && tile_y == 22594 {
    //     eprintln!("{tile_id}");
    // }
    // if tile_id == 1754 {
    //     let mut hashed_ways: HashMap<(i32, i32), Vec<(WayId, [[u8; 2]; 2])>> = HashMap::new();
    //     for global_way_id in tile_ways.iter() {
    //         let mut new_way = Vec::new();
    //         for node in ways[*global_way_id as usize]
    //             .iter()
    //             .map(|&i| &nodes[i as usize])
    //         {
    //             let new_node = node.encode(tile_x, tile_y, side);
    //             new_way.push(new_node);
    //         }
    //         if new_way[0] > new_way[1] {
    //             new_way.reverse();
    //         }
    //         let [x1, y1] = new_way[0];
    //         let [x2, y2] = new_way[1];
    //         let x1 = x1 as f64;
    //         let y1 = y1 as f64;
    //         let x2 = x2 as f64;
    //         let y2 = y2 as f64;
    //         let key = if x1 == x2 {
    //             (std::i32::MAX, x1 as i32)
    //         } else {
    //             let slope = (y2 - y1) / (x2 - x1);
    //             let integer_slope = (slope * 8.).floor() as i32;
    //             let y_origin = y1 - x1 * slope;
    //             (integer_slope, y_origin.floor() as i32)
    //         };
    //         hashed_ways
    //             .entry(key)
    //             .or_default()
    //             .push((*global_way_id, [new_way[0], new_way[1]]));
    //     }
    //     for (key, mut aligned_ways) in hashed_ways {
    //         aligned_ways.sort_by_key(|&(_, [[x, _], _])| x);
    //         eprintln!("{key:?} aligned: {:?}", aligned_ways);
    //     }
    //     todo!()
    // }
    for (local_way_id, global_way_id) in tile_ways.iter().enumerate() {
        let mut new_way = Vec::new();
        for node in ways[*global_way_id as usize]
            .iter()
            .map(|&i| &nodes[i as usize])
        {
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
        // if tile_id == 1754 {
        //     eprintln!("way: {new_way:?}");
        // }
        if new_way[0][0] != new_way[1][0] || new_way[0][1] != new_way[1][1] {
            compressed_ways.push([new_way[0], new_way[1]]);
        }
    }

    compressed_ways
}
