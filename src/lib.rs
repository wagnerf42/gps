use itertools::Itertools;
use std::collections::HashMap;

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

pub fn group_segments_in_squares(
    nodes: &Vec<Node>,
    ways: &HashMap<usize, Vec<usize>>,
    first_square_coordinates: (f64, f64),
    squares_per_line: usize,
    side: f64,
) -> (Vec<((u8, u8), (u8, u8))>, Vec<usize>) {
    let mut squares: HashMap<(usize, usize), Vec<(u8, u8)>> = HashMap::new();
    ways.values()
        .flat_map(|way| way.iter().tuple_windows())
        .map(|(id1, id2)| (nodes[*id1], nodes[*id2]))
        .map(|(n1, n2)| {
            let xk1 = n1.x / side;
            let yk1 = n1.y / side;

            todo!()
        });
    todo!()
}

pub fn group_nodes_in_squares(
    nodes: &mut Vec<Node>,
    ways: &mut HashMap<usize, Vec<usize>>,
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
    for way in ways.values_mut() {
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

pub fn cut_ways_at_squares(
    nodes: &mut Vec<Node>,
    ways: &mut HashMap<usize, Vec<usize>>,
    side: f64,
) {
    for way in ways.values_mut() {
        cut_way_at_squares(nodes, way, side)
    }
}

pub fn cut_way_at_squares(nodes: &mut Vec<Node>, way: &mut Vec<usize>, side: f64) {
    // we assume ways never intersect between their endpoints

    let mut new_way = Vec::new();
    for (i1, i2) in way.iter().copied().tuples() {
        new_way.push(i1);
        let n1 = nodes[i1];
        let n2 = nodes[i2];

        let mut new_nodes = grid_coordinates_between(n1.x, n2.x, side)
            .map(|x| vertical_segment_intersection(&n1, &n2, x))
            .chain(
                grid_coordinates_between(n1.y, n2.y, side)
                    .map(|y| horizontal_segment_intersection(&n1, &n2, y)),
            )
            .collect::<Vec<_>>();
        new_nodes.sort_unstable_by(|na, nb| {
            let da = na.squared_distance_between(&n1);
            let db = nb.squared_distance_between(&n1);
            da.partial_cmp(&db).unwrap()
        });
        for new_node in new_nodes.into_iter().dedup() {
            let new_id = nodes.len();
            new_way.push(new_id);
            nodes.push(new_node);
        }
    }
    new_way.extend(way.last().copied());
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
fn grid_coordinates_between(mut start: f64, mut end: f64, side: f64) -> impl Iterator<Item = f64> {
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
