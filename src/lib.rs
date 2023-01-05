use itertools::Itertools;
use std::collections::{HashMap, HashSet};

mod node;
pub use node::Node;
mod osm;
mod simplify;
pub use osm::{parse_osm_xml, request};
mod utils;
pub use utils::grid_coordinates_between;
mod compression;
pub use compression::CompressedMap;
mod graph;

pub type TileKey = (usize, usize);
pub type WayId = usize;
pub type NodeId = usize;

// convert osm nodes ids to smaller integers (from 0 to nodes number)
// and update ways accordingly
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
            let da = na.squared_distance_between(&n1);
            let db = nb.squared_distance_between(&n1);
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
            .flat_map(|way_id| ids_changes[way_id].iter())
            .copied()
            .collect::<Vec<_>>();
        *street_ways = new_street_ways;
    }
    new_ways
}

// cut ways such that each fits a single tile.
pub fn cut_ways_on_tiles(
    nodes: &[Node],
    ways: Vec<Vec<NodeId>>,
    streets: &mut HashMap<String, Vec<WayId>>,
    side: f64,
) -> (Vec<Vec<NodeId>>, HashMap<TileKey, Vec<WayId>>) {
    let mut new_ways = Vec::new();
    let mut ids_changes: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut tiles_ways: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (old_way_id, way) in ways.into_iter().enumerate() {
        assert!(way.len() > 1);
        way.into_iter()
            .map(|id| (id, &nodes[id]))
            .multipeek()
            .batching(|it| {
                if let Some((first_node_id, first_node)) = it.next() {
                    let mut smaller_way = vec![first_node_id];
                    let mut current_tiles = first_node.tiles(side).collect::<HashSet<_>>();
                    let mut tile_id = *current_tiles.iter().next().unwrap();
                    while let Some((next_node_id, next_node)) = it.peek() {
                        smaller_way.push(*next_node_id);
                        current_tiles = current_tiles
                            .intersection(&next_node.tiles(side).collect::<HashSet<_>>())
                            .copied()
                            .collect();
                        tile_id = *current_tiles.iter().next().unwrap();
                        if let Some((_, next_node)) = it.peek() {
                            current_tiles = current_tiles
                                .intersection(&next_node.tiles(side).collect::<HashSet<_>>())
                                .copied()
                                .collect();
                            if current_tiles.is_empty() {
                                return Some((tile_id, smaller_way));
                            } else {
                                it.next();
                            }
                        } else {
                            it.next();
                        }
                    }
                    Some((tile_id, smaller_way))
                } else {
                    None
                }
            })
            .for_each(|(tile_id, smaller_way)| {
                assert!(smaller_way.len() > 1);
                let new_id = new_ways.len();
                new_ways.push(smaller_way);
                ids_changes.entry(old_way_id).or_default().push(new_id);
                tiles_ways.entry(tile_id).or_default().push(new_id);
            })
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

    (new_ways, tiles_ways)
}
