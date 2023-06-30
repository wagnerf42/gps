use itertools::Itertools;
use std::collections::HashMap;
use xml::{reader::XmlEvent, EventReader};

use crate::{Node, NodeId, WayId};

pub async fn request(polygon: &[Node]) -> Result<String, Box<dyn std::error::Error>> {
    let polygon_string: String = polygon
        .iter()
        .flat_map(|n| [n.y, n.x])
        .inspect(|c| assert!(!c.is_nan()))
        .join(" ");
    let query = format!(
        "(
        way[\"highway\"][\"highway\"!=\"motorway\"][\"highway\"!=\"trunk\"][\"hightway\"!=\"motorway_link\"][\"highway\"!=\"trunk_link\"][\"footway\"!=\"crossing\"][\"area\"!=\"yes\"](poly:\"{polygon_string}\");
        >;
        node(poly:\"{polygon_string}\");
        );
        out body;",
    );
    eprintln!("request: {polygon_string:?}");
    let client = reqwest::Client::builder()
        //.user_agent("osm-geo-mapper")
        .build()?;
    let response = client
        .post("https://overpass-api.de/api/interpreter")
        .body(query)
        .send()
        .await?;
    let result = response.text().await?;
    Ok(result)
}

// return a hash map point id -> point
// and a hash map way id -> vec of points id in the way
// and a hash map street name -> Vec of ways ids
pub fn parse_osm_xml(
    xml: &str,
    key_values: &[(String, String)],
) -> (
    HashMap<NodeId, Node>,
    HashMap<WayId, Vec<NodeId>>,
    HashMap<String, Vec<WayId>>,
    Vec<(usize, Node)>, // interests (type + node)
) {
    let key_values: HashMap<(&String, &String), usize> = key_values
        .iter()
        .enumerate()
        .map(|(i, (key, value))| ((key, value), i + 1))
        .collect();
    let parser = EventReader::new(xml.as_bytes());
    let mut current_node = None;
    let mut current_way: Option<(WayId, Vec<NodeId>)> = None;
    let mut nodes = HashMap::new();
    let mut ways = HashMap::new();
    let mut streets: HashMap<String, Vec<WayId>> = HashMap::new();
    let mut interests = Vec::new();
    let mut current_interest = None;
    let mut footway = false;
    let mut bicycle = false;
    let mut discard_way = false;
    let mut current_street_name = None;
    for e in parser {
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                if name.local_name == "way" {
                    footway = false;
                    bicycle = false;
                    discard_way = false;
                    current_way = attributes.iter().find_map(|a| {
                        if a.name.local_name == "id" {
                            a.value.parse::<u64>().ok().map(|id| (id, Vec::new()))
                        } else {
                            None
                        }
                    });
                }
                if name.local_name == "tag" {
                    let mut key = None;
                    let mut value = None;
                    for attribute in &attributes {
                        if attribute.name.local_name == "k" {
                            key = Some(&attribute.value)
                        } else if attribute.name.local_name == "v" {
                            value = Some(&attribute.value)
                        }
                    }

                    if let Some(key) = key {
                        if let Some(value) = value {
                            if key == "bicycle" && value == "yes" {
                                bicycle = true;
                            }
                            if key == "highway" && value == "footway" {
                                footway = true;
                            }
                            if key == "highway" && (value == "raceway" || value == "steps") {
                                discard_way = true;
                            }
                            if let Some(interest_id) = key_values.get(&(key, value)) {
                                current_interest = Some(*interest_id);
                            }
                        }
                    }

                    if key == Some(&"name".to_owned()) {
                        current_street_name = value.map(|s| s.to_owned());
                    }
                }
                if name.local_name == "nd" {
                    if let Some((_, points)) = current_way.as_mut() {
                        points.extend(attributes.iter().find_map(|a| {
                            if a.name.local_name == "ref" {
                                a.value.parse::<u64>().ok()
                            } else {
                                None
                            }
                        }))
                    }
                }
                if name.local_name == "node" {
                    current_interest = None;
                    let mut lon = None;
                    let mut lat = None;
                    let mut id = None;
                    for attribute in &attributes {
                        if attribute.name.local_name == "lon" {
                            lon = attribute.value.parse::<f64>().ok();
                        } else if attribute.name.local_name == "lat" {
                            lat = attribute.value.parse::<f64>().ok();
                        } else if attribute.name.local_name == "id" {
                            id = attribute.value.parse::<u64>().ok();
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
                        if !discard_way && (!footway || bicycle) {
                            ways.insert(id, way_points);

                            if let Some(street_name) = current_street_name.take() {
                                streets.entry(street_name.to_owned()).or_default().push(id)
                            }
                        }
                    }
                }
                if name.local_name == "node" {
                    if let Some((id, node)) = current_node.take() {
                        nodes.insert(id, node);
                        if let Some(interest) = current_interest {
                            interests.push((interest, node));
                        }
                    }
                }
            }
            Err(e) => {
                println!("Error: {e}");
                break;
            }
            _ => {}
        }
    }
    (nodes, ways, streets, interests)
}
