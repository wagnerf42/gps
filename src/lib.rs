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
    Vec<(f64, f64)>,       // nodes stored in squares but flattened
    HashMap<usize, usize>, // osm node id -> new index in vec
    Vec<usize>,            // start index of each first node in each bucket
    usize,                 // number of squares per line
) {
    let (xmin, ymin, xmax, ymax) = bbox;
    let mut squares: HashMap<(u32, u32), Vec<(f64, f64)>> = HashMap::new();
    let mut ids_semi_translation = HashMap::new();
    for (id, &(x, y)) in nodes.iter().enumerate() {
        if x < xmin || x > xmax || y < ymin || y > ymax {
            continue;
        }
        let square_id = (
            ((y - ymin) / side).floor() as u32,
            ((x - xmin) / side).floor() as u32,
        );
        let v = squares.entry(square_id).or_default();
        v.push((x, y));
        ids_semi_translation.insert(id, (square_id, v.len()));
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
    eprintln!(
        "we have {} squares, {} per line",
        first_ones.len(),
        squares_per_line
    );
    let mut ids_translation = HashMap::new();
    for (osm_id, ((square_y, square_x), inner_pos)) in ids_semi_translation {
        let square_index = square_y as usize * squares_per_line + square_x as usize;
        let real_id = first_ones[square_index] + inner_pos;
        ids_translation.insert(osm_id, real_id);
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
