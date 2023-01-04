use itertools::Itertools;
use std::collections::{HashMap, HashSet};

use xml::{reader::XmlEvent, EventReader};

mod simplify;
pub use simplify::Node;

pub async fn request(
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
) -> Result<String, Box<dyn std::error::Error>> {
    let query = format!(
        "https://overpass-api.de/api/interpreter?data=
        [bbox: {}, {}, {}, {}];
        (
        way[\"highway\"][\"highway\"!=\"motorway\"][\"highway\"!=\"trunk\"];
        >;
        );
        out body;",
        ymin, xmin, ymax, xmax
    );
    let client = reqwest::Client::builder()
        //.user_agent("osm-geo-mapper")
        .build()?;
    let response = client.get(&query).send().await?;
    let result = response.text().await?;
    Ok(result)
}

// return a hash map point id -> point
// and a hash map way id -> vec of points id in the way
// and a hash map street name -> Vec of ways ids
pub fn parse_osm_xml(
    xml: &str,
) -> (
    HashMap<usize, Node>,
    HashMap<usize, Vec<usize>>,
    HashMap<String, Vec<usize>>,
) {
    let parser = EventReader::new(xml.as_bytes());
    let mut current_node = None;
    let mut current_way: Option<(usize, Vec<usize>)> = None;
    let mut nodes = HashMap::new();
    let mut ways = HashMap::new();
    let mut streets: HashMap<String, Vec<usize>> = HashMap::new();
    for e in parser {
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                if name.local_name == "way" {
                    current_way = attributes.iter().find_map(|a| {
                        if a.name.local_name == "id" {
                            a.value.parse::<usize>().ok().map(|id| (id, Vec::new()))
                        } else {
                            None
                        }
                    })
                }
                if name.local_name == "tag" {
                    let mut named = false;
                    let mut name = None;
                    for attribute in &attributes {
                        if attribute.name.local_name == "k" && attribute.value == "name" {
                            named = true;
                        } else if attribute.name.local_name == "v" {
                            name = Some(&attribute.value)
                        }
                    }
                    if named {
                        if let Some(street_name) = name {
                            if let Some((way, _)) = current_way.as_ref() {
                                streets
                                    .entry(street_name.to_owned())
                                    .or_default()
                                    .push(*way)
                            }
                        }
                    }
                }
                if name.local_name == "nd" {
                    if let Some((_, points)) = current_way.as_mut() {
                        points.extend(attributes.iter().find_map(|a| {
                            if a.name.local_name == "ref" {
                                a.value.parse::<usize>().ok()
                            } else {
                                None
                            }
                        }))
                    }
                }
                if name.local_name == "node" {
                    let mut lon = None;
                    let mut lat = None;
                    let mut id = None;
                    for attribute in &attributes {
                        if attribute.name.local_name == "lon" {
                            lon = attribute.value.parse::<f64>().ok();
                        } else if attribute.name.local_name == "lat" {
                            lat = attribute.value.parse::<f64>().ok();
                        } else if attribute.name.local_name == "id" {
                            id = attribute.value.parse::<usize>().ok();
                        }
                    }
                    if let Some(lon) = lon {
                        if let Some(lat) = lat {
                            if let Some(id) = id {
                                current_node = Some((id, Node::new(lon, lat)))
                            }
                        }
                    }
                }
            }
            Ok(XmlEvent::EndElement { name }) => {
                if name.local_name == "way" {
                    if let Some((id, way_points)) = current_way.take() {
                        ways.insert(id, way_points);
                    }
                }
                if name.local_name == "node" {
                    if let Some((id, node)) = current_node.take() {
                        nodes.insert(id, node);
                    }
                }
            }
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
            _ => {}
        }
    }
    (nodes, ways, streets)
}

pub fn group_nodes_in_squares(
    nodes: &mut Vec<Node>,
    ways: &mut [Vec<usize>],
    side: f64,
) -> (
    Vec<usize>, // start index of each first node in each bucket
    usize,      // number of squares per line
    (f64, f64), // coordinates of first square
) {
    // put every point in it's square
    let mut squares: HashMap<(usize, usize), Vec<(usize, Node)>> = HashMap::new();
    for (old_id, &n) in nodes.iter().enumerate() {
        let square_id = ((n.y / side).floor() as usize, (n.x / side).floor() as usize);
        squares.entry(square_id).or_default().push((old_id, n));
    }

    let mut squares_keys = squares.keys().copied().collect::<Vec<_>>();
    squares_keys.sort_unstable();
    let (min_x, max_x) = squares_keys
        .iter()
        .copied()
        .map(|(_, x)| x)
        .minmax()
        .into_option()
        .unwrap();
    let min_y = squares_keys[0].0;
    let max_y = squares_keys.last().map(|(y, _)| *y).unwrap();
    let mut nodes_in_squares = Vec::with_capacity(nodes.len());
    let mut squares_starts = Vec::with_capacity((max_y + 1 - min_y) * (max_x + 1 - min_x));
    let mut ids_translations = HashMap::with_capacity(nodes.len());
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let square_content = squares.get(&(y, x));
            squares_starts.push(nodes_in_squares.len());
            for (old_id, node) in square_content.into_iter().flatten().copied() {
                let new_id = nodes_in_squares.len();
                nodes_in_squares.push(node);
                ids_translations.insert(old_id, new_id);
            }
        }
    }
    for way in ways.iter_mut() {
        for old_id in way {
            *old_id = ids_translations[old_id];
        }
    }
    std::mem::swap(nodes, &mut nodes_in_squares);
    (
        squares_starts,
        max_x + 1 - min_x,
        (min_x as f64 * side, min_y as f64 * side),
    )
}

// convert osm nodes ids to smaller integers (from 0 to nodes number)
// and update ways accordingly
pub fn rename_nodes(
    nodes: HashMap<usize, Node>,
    ways: &mut HashMap<usize, Vec<usize>>,
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

pub fn cut_segments_on_tiles(nodes: &mut Vec<Node>, ways: &mut [Vec<usize>], side: f64) {
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
    nodes_ids: &mut HashMap<Node, usize>,
    way: &mut Vec<usize>,
    side: f64,
) {
    let mut new_way = Vec::new();
    for (i1, i2) in way.iter().copied().tuple_windows() {
        let n1 = nodes[i1];
        let n2 = nodes[i2];

        let mut new_nodes = std::iter::once(n1)
            .chain(
                grid_coordinates_between(n1.x, n2.x, side)
                    .map(|x| vertical_segment_intersection(&n1, &n2, x))
                    .chain(
                        grid_coordinates_between(n1.y, n2.y, side)
                            .map(|y| horizontal_segment_intersection(&n1, &n2, y)),
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

fn horizontal_segment_intersection(n1: &Node, n2: &Node, y: f64) -> Node {
    let fraction_of_segment = (y - n1.y) / (n2.y - n1.y);
    let x = n1.x + fraction_of_segment * (n2.x - n1.x);
    Node::new(x, y)
}

fn vertical_segment_intersection(n1: &Node, n2: &Node, x: f64) -> Node {
    let fraction_of_segment = (x - n1.x) / (n2.x - n1.x);
    let y = n1.y + fraction_of_segment * (n2.y - n1.y);
    Node::new(x, y)
}

// loop on all coordinates c intersecting grid at min + side * alpha
// such that start < c < end
pub fn grid_coordinates_between(
    mut start: f64,
    mut end: f64,
    side: f64,
) -> impl Iterator<Item = f64> {
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    let start_cell = start / side;
    let above_start_cell = start_cell.ceil();
    let real_start_cell = if start_cell == above_start_cell {
        (above_start_cell + 1.) as u32
    } else {
        above_start_cell as u32
    };
    let end_cell = (end / side).ceil() as u32;
    (real_start_cell..end_cell).map(move |alpha| alpha as f64 * side)
}

pub fn simplify_ways(
    nodes: &mut Vec<Node>,
    ways: &mut Vec<Vec<usize>>,
    streets: &mut HashMap<String, Vec<usize>>,
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

fn compute_node_degrees(ways: &HashMap<usize, Vec<usize>>) -> HashMap<usize, usize> {
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
    ways: HashMap<usize, Vec<usize>>,
    streets: &mut HashMap<String, Vec<usize>>,
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

pub fn cut_ways_on_tiles(
    nodes: &[Node],
    ways: Vec<Vec<usize>>,
    streets: &mut HashMap<String, Vec<usize>>,
    side: f64,
) -> Vec<Vec<usize>> {
    let mut new_ways = Vec::new();
    let mut ids_changes: HashMap<usize, Vec<usize>> = HashMap::new();
    for (old_way_id, way) in ways.into_iter().enumerate() {
        assert!(way.len() > 1);
        way.into_iter()
            .map(|id| (id, &nodes[id]))
            .multipeek()
            .batching(|it| {
                if let Some((first_node_id, first_node)) = it.next() {
                    let mut smaller_way = vec![first_node_id];
                    let mut current_tiles = tiles(first_node, side).collect::<HashSet<_>>();
                    while let Some((next_node_id, next_node)) = it.peek() {
                        smaller_way.push(*next_node_id);
                        current_tiles = current_tiles
                            .intersection(&tiles(next_node, side).collect::<HashSet<_>>())
                            .copied()
                            .collect();
                        if let Some((_, next_node)) = it.peek() {
                            current_tiles = current_tiles
                                .intersection(&tiles(next_node, side).collect::<HashSet<_>>())
                                .copied()
                                .collect();
                            if current_tiles.is_empty() {
                                return Some(smaller_way);
                            } else {
                                it.next();
                            }
                        } else {
                            it.next();
                        }
                    }
                    Some(smaller_way)
                } else {
                    None
                }
            })
            .for_each(|smaller_way| {
                assert!(smaller_way.len() > 1);
                let new_id = new_ways.len();
                new_ways.push(smaller_way);
                ids_changes.entry(old_way_id).or_default().push(new_id);
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

    new_ways
}

// Loop on all tiles the node belongs.
fn tiles(node: &Node, side: f64) -> impl Iterator<Item = (usize, usize)> {
    let x = node.x / side;
    let y = node.y / side;
    let x_key = x.floor() as usize;
    let y_key = y.floor() as usize;
    let x_key_2 = x.ceil() as usize;
    let y_key_2 = y.ceil() as usize;
    let left = (x_key == x_key_2).then_some((x_key - 1, y_key));
    let top = (y_key == y_key_2).then_some((x_key, y_key - 1));
    let top_left = ((x_key == x_key_2) && (y_key == y_key_2)).then_some((x_key - 1, y_key - 1));
    std::iter::once((x_key, y_key))
        .chain(left)
        .chain(top)
        .chain(top_left)
}
