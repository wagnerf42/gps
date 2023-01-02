use itertools::Itertools;
use std::collections::HashMap;

use xml::{reader::XmlEvent, EventReader};

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
    HashMap<usize, (f64, f64)>,
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
                                current_node = Some((id, lon, lat))
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
                    if let Some((id, lon, lat)) = current_node.take() {
                        nodes.insert(id, (lon, lat));
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
    nodes: &[(f64, f64)],
    bbox: (f64, f64, f64, f64),
    side: f64,
) -> (
    Vec<(f64, f64)>, // nodes stored in squares but flattened
    Vec<usize>,      // new indices in vec
    Vec<usize>,      // start index of each first node in each bucket
    usize,           // number of squares per line
) {
    let (xmin, ymin, xmax, ymax) = bbox;
    let mut squares: HashMap<(u32, u32), Vec<(f64, f64)>> = HashMap::new();
    let mut ids_semi_translation = Vec::new();
    for &(x, y) in nodes.iter() {
        if x < xmin || x > xmax || y < ymin || y > ymax {
            continue;
        }
        let square_id = (
            ((y - ymin) / side).floor() as u32,
            ((x - xmin) / side).floor() as u32,
        );
        let v = squares.entry(square_id).or_default();
        v.push((x, y));
        ids_semi_translation.push((square_id, v.len()));
    }
    let mut nodes: Vec<(f64, f64)> = Vec::new();
    let mut first_ones = Vec::new();
    let x_keys = 0..=(((xmax - xmin) / side).floor() as u32);
    let y_keys = 0..=(((ymax - ymin) / side).floor() as u32);
    let squares_per_line = ((xmax - xmin) / side).floor() as usize + 1;
    for id in y_keys.cartesian_product(x_keys) {
        first_ones.push(nodes.len());
        nodes.extend(squares.get(&id).into_iter().flatten());
    }
    let mut ids_translation = Vec::new();
    for ((square_y, square_x), inner_pos) in ids_semi_translation {
        let square_index = square_y as usize * squares_per_line + square_x as usize;
        let real_id = first_ones[square_index] + inner_pos;
        ids_translation.push(real_id);
    }
    (nodes, ids_translation, first_ones, squares_per_line)
}

// convert osm nodes ids to smaller integers (from 0 to nodes number)
// and update ways accordingly
pub fn rename_nodes(
    nodes: HashMap<usize, (f64, f64)>,
    ways: &mut HashMap<usize, Vec<usize>>,
) -> Vec<(f64, f64)> {
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
    nodes: &mut Vec<(f64, f64)>,
    ways: &mut HashMap<usize, Vec<usize>>,
    grid_origin: (f64, f64),
    side: f64,
) {
    for way in ways.values_mut() {
        cut_way_at_squares(nodes, way, grid_origin, side)
    }
}

pub fn cut_way_at_squares(
    nodes: &mut Vec<(f64, f64)>,
    way: &mut Vec<usize>,
    grid_origin: (f64, f64),
    side: f64,
) {
    // we assume ways never intersect between their endpoints

    let (xmin, ymin) = grid_origin;
    let mut new_way = Vec::new();
    for (i1, i2) in way.iter().copied().tuples() {
        new_way.push(i1);
        let (x1, y1) = nodes[i1];
        let (x2, y2) = nodes[i2];

        let mut new_nodes = grid_coordinates_between(xmin, x1, x2, side)
            .map(|x| vertical_segment_intersection(x1, y1, x2, y2, x))
            .chain(
                grid_coordinates_between(ymin, y1, y2, side)
                    .map(|y| horizontal_segment_intersection(x1, y1, x2, y2, y)),
            )
            .collect::<Vec<_>>();
        new_nodes.sort_unstable_by(|(xa, ya), (xb, yb)| {
            let da = (xa - x1) * (xa - x1) + (ya - y1) * (ya - y1);
            let db = (xb - x1) * (xb - x1) + (yb - y1) * (yb - y1);
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

fn horizontal_segment_intersection(x1: f64, y1: f64, x2: f64, y2: f64, y: f64) -> (f64, f64) {
    let fraction_of_segment = (y - y1) / (y2 - y1);
    let x = x1 + fraction_of_segment * (x2 - x1);
    (x, y)
}

fn vertical_segment_intersection(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> (f64, f64) {
    let fraction_of_segment = (x - x1) / (x2 - x1);
    let y = y1 + fraction_of_segment * (y2 - y1);
    (x, y)
}

// loop on all coordinates c intersecting grid at min + side * alpha
// such that start < c < end
fn grid_coordinates_between(
    min: f64,
    mut start: f64,
    mut end: f64,
    side: f64,
) -> impl Iterator<Item = f64> {
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    let start_cell = (start - min) / side;
    let above_start_cell = start_cell.ceil();
    let real_start_cell = if start_cell == above_start_cell {
        (above_start_cell + 1.) as u32
    } else {
        above_start_cell as u32
    };
    let end_cell = ((end - min) / side).ceil() as u32;
    (real_start_cell..end_cell).map(move |alpha| min + alpha as f64 * side)
}
