use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::*;

mod gps;
pub use gps::{
    get_gps_content, get_polygon, gps_from_area, load_gps_from_file, load_gps_from_string,
    request_map, Gps,
};
mod node;
pub use node::Node;
// mod geometry;
// pub use geometry::inflate_polyline;
mod osm;
pub use osm::{parse_osm_xml, request};
mod simplify;
pub use simplify::simplify_path;
mod utils;
pub use utils::grid_coordinates_between;
pub mod map;
pub use map::{load_map_and_interests, map_and_interests_from_string, Map, SIDE};
mod graph;
mod svg;
pub use svg::{save_svg, Svg, SvgW};
mod gpx;
pub use crate::gpx::{detect_sharp_turns, parse_gpx_points, request_map_from};
mod interests;
mod streets;
pub use interests::save_tiled_interests;

pub type TileKey = (usize, usize);
pub type WayId = usize;
pub type NodeId = usize;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct CWayId {
    pub(crate) tile_number: u16,
    pub(crate) local_way_id: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CNodeId {
    tile_number: u16,
    local_node_id: u16,
}

pub const TILE_BORDER_THICKNESS: f64 = 1. / 111_200.;

pub fn rename_nodes(
    nodes: HashMap<NodeId, Node>,
    ways: &mut HashMap<WayId, Vec<NodeId>>,
) -> Vec<Node> {
    let mut new_ids = HashMap::new();
    let mut renamed_nodes = Vec::new();
    for way in ways.values_mut() {
        for node in way {
            let new_id = new_ids.entry(*node).or_insert_with(|| {
                let id = renamed_nodes.len();
                renamed_nodes.push(nodes[node]);
                id
            });
            *node = *new_id
        }
    }
    renamed_nodes
}

// add extra nodes when a segment crosses between two tiles
// such that the cross point appears.
pub fn cut_segments_on_tiles(nodes: &mut Vec<Node>, ways: &mut [Vec<NodeId>], side: f64) {
    let mut nodes_ids = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (*n, i))
        .collect::<HashMap<_, _>>();
    for way in ways.iter_mut() {
        cut_way_segments_on_tiles(nodes, &mut nodes_ids, way, side)
    }
}

pub fn cut_way_segments_on_tiles(
    nodes: &mut Vec<Node>,
    nodes_ids: &mut HashMap<Node, NodeId>,
    way: &mut Vec<NodeId>,
    side: f64,
) {
    let mut new_way = Vec::new();
    for (i1, i2) in way.iter().copied().tuple_windows() {
        let n1 = nodes[i1];
        let n2 = nodes[i2];

        let mut new_nodes = std::iter::once(n1)
            .chain(
                grid_coordinates_between(n1.x, n2.x, side)
                    .map(|x| n1.vertical_segment_intersection(&n2, x))
                    .chain(
                        grid_coordinates_between(n1.y, n2.y, side)
                            .map(|y| n1.horizontal_segment_intersection(&n2, y)),
                    ),
            )
            .chain(std::iter::once(n2))
            .collect::<Vec<_>>();
        new_nodes.sort_unstable_by(|na, nb| {
            let da = na.squared_distance_to(&n1);
            let db = nb.squared_distance_to(&n1);
            da.partial_cmp(&db).unwrap()
        });
        for new_node in new_nodes.into_iter().dedup() {
            let new_id = *nodes_ids.entry(new_node).or_insert_with(|| {
                let id = nodes.len();
                nodes.push(new_node);
                id
            });
            new_way.push(new_id);
        }
    }
    assert!(new_way.len() > 1);
    new_way.dedup();
    assert!(new_way.len() > 1);
    *way = new_way
}

// apply simplification algorithm on each way to reduce number of nodes.
// pre-conditions:
//    * no node of degree >= 2 strictly inside the way.
//    * if a segment crosses between tiles there is always a cross node belonging to both tiles
pub fn simplify_ways(
    nodes: &mut Vec<Node>,
    ways: &mut Vec<Vec<NodeId>>,
    streets: &mut HashMap<String, Vec<WayId>>,
) {
    let mut new_nodes = HashMap::new();
    let mut new_ways = Vec::new();
    let mut new_nodes_vec = Vec::new();
    let mut ids_changes: HashMap<usize, usize> = HashMap::new();
    for (old_way_id, way) in ways.iter().enumerate() {
        assert!(way.len() > 1);
        let way_nodes = way.iter().map(|id| nodes[*id]).collect::<Vec<_>>();
        let simpler_way_nodes = simplify::simplify_path(&way_nodes, 0.00015);
        let new_way = simpler_way_nodes
            .into_iter()
            .map(|new_node| {
                let id = *new_nodes.entry(new_node).or_insert_with(|| {
                    let id = new_nodes_vec.len();
                    new_nodes_vec.push(new_node);
                    id
                });
                id
            })
            .dedup()
            .collect::<Vec<_>>();
        if new_way.len() > 1 {
            let new_way_id = new_ways.len();
            new_ways.push(new_way); // very small loops might disappear
            ids_changes.insert(old_way_id, new_way_id);
        }
    }
    std::mem::swap(&mut new_ways, ways);
    std::mem::swap(&mut new_nodes_vec, nodes);
    for street in streets.values_mut() {
        let new_street = street
            .iter()
            .filter_map(|old_id| ids_changes.get(old_id))
            .copied()
            .collect::<Vec<_>>();
        *street = new_street;
    }
    streets.retain(|_, s| !s.is_empty());
}

fn compute_node_degrees(ways: &HashMap<WayId, Vec<NodeId>>) -> HashMap<NodeId, usize> {
    let mut degrees: HashMap<usize, usize> = HashMap::new();
    for id in ways.values().flat_map(|way| way.iter()).copied() {
        *degrees.entry(id).or_default() += 1;
    }
    degrees
}

// ensure no node of degree >= 2 is strictly inside a way but cutting ways
// into smaller parts.
// we also renumber ways to get integers from 0 to ways_num and return them as a vector.
pub fn sanitize_ways(
    ways: HashMap<WayId, Vec<NodeId>>,
    streets: &mut HashMap<String, Vec<WayId>>,
) -> Vec<Vec<usize>> {
    let degrees = compute_node_degrees(&ways);
    let mut new_ways = Vec::new();
    let mut ids_changes: HashMap<usize, Vec<usize>> = HashMap::new();

    // first, cut the ways
    for (way_id, way) in ways {
        for small_way in way.into_iter().peekable().batching(|it| {
            let mut small_way = it.next().into_iter().collect::<Vec<_>>();
            while let Some(id) = it.peek() {
                if degrees[id] > 1 {
                    small_way.push(*id);
                    return Some(small_way);
                } else {
                    small_way.extend(it.next());
                }
            }
            if small_way.len() <= 1 {
                None
            } else {
                Some(small_way)
            }
        }) {
            let new_id = new_ways.len();
            assert!(small_way.len() > 1);
            new_ways.push(small_way);
            ids_changes.entry(way_id).or_default().push(new_id);
        }
    }

    // now update the streets
    for street_ways in streets.values_mut() {
        let new_street_ways = street_ways
            .iter()
            .filter_map(|way_id| ids_changes.get(way_id))
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        *street_ways = new_street_ways;
    }
    new_ways
}

// cut ways such that we only get segments.
pub fn cut_ways_into_edges(
    ways: Vec<Vec<NodeId>>,
    streets: &mut HashMap<String, Vec<WayId>>,
) -> Vec<[NodeId; 2]> {
    let mut new_ways = Vec::new();
    let mut ids_changes: HashMap<usize, Vec<usize>> = HashMap::new();
    for (old_way_id, way) in ways.into_iter().enumerate() {
        for (n1, n2) in way.into_iter().tuple_windows() {
            let new_way_id = new_ways.len();
            new_ways.push([n1, n2]);
            ids_changes.entry(old_way_id).or_default().push(new_way_id);
        }
    }

    // now update the streets
    for street_ways in streets.values_mut() {
        let new_street_ways = street_ways
            .iter()
            .flat_map(|way_id| ids_changes[way_id].iter())
            .copied()
            .collect::<Vec<_>>();
        *street_ways = new_street_ways;
    }

    new_ways
}

pub fn group_ways_in_tiles(
    nodes: &[Node],
    ways: &[[NodeId; 2]],
    side: f64,
) -> HashMap<TileKey, Vec<WayId>> {
    let mut tiles: HashMap<TileKey, Vec<WayId>> = HashMap::new();
    for (way_id, [n1, n2]) in ways.iter().enumerate() {
        let tile_id = nodes[*n1]
            .tiles(side)
            .collect::<HashSet<_>>()
            .intersection(&nodes[*n2].tiles(side).collect::<HashSet<_>>())
            .next()
            .copied()
            .unwrap();
        tiles.entry(tile_id).or_default().push(way_id);
    }
    tiles
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[cfg(not(target_arch = "wasm32"))]
fn log(s: &str) {}
