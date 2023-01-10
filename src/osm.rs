use std::collections::HashMap;
use xml::{reader::XmlEvent, EventReader};

use crate::{Node, NodeId, WayId};

pub async fn request(
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
) -> Result<String, Box<dyn std::error::Error>> {
    let query = format!(
        "https://overpass-api.de/api/interpreter?data=
        [bbox: {ymin}, {xmin}, {ymax}, {xmax}];
        (
        way[\"highway\"][\"highway\"!=\"motorway\"][\"highway\"!=\"trunk\"][\"hightway\"!=\"motorway_link\"][\"highway\"!=\"trunk_link\"][\"footway\"!=\"crossing\"][\"area\"!=\"yes\"];
        >;
        );
        out body;",
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
    HashMap<NodeId, Node>,
    HashMap<WayId, Vec<NodeId>>,
    HashMap<String, Vec<WayId>>,
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
                println!("Error: {e}");
                break;
            }
            _ => {}
        }
    }
    (nodes, ways, streets)
}
