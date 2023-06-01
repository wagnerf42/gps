use itertools::Itertools;
use std::{
    collections::HashSet,
    io::{Read, Write},
};
use wasm_bindgen::prelude::*;

use crate::{
    detect_sharp_turns, gpx::save_path, parse_gpx_points, save_svg, save_tiled_interests,
    simplify_path, svg::UniColorNodes, Map, Node, SvgW,
};

#[wasm_bindgen]
pub struct Gps {
    path: Option<Vec<Node>>,
    waypoints: Option<HashSet<Node>>,
    map_polygon: Vec<Node>,
    interests: Vec<(usize, Node)>,
    map: Option<Map>,
}

#[wasm_bindgen]
pub fn get_polygon(gps: &Gps) -> Vec<f64> {
    gps.map_polygon.iter().flat_map(|n| [n.y, n.x]).collect()
}

#[wasm_bindgen]
pub fn get_gps_content(gps: &Gps) -> Vec<u8> {
    let mut binary: Vec<u8> = Vec::new();
    gps.write_gps(&mut binary).expect("failed writing binary");
    binary
}

#[wasm_bindgen]
pub async fn request_map(
    gps: &mut Gps,
    key1: &str,
    value1: &str,
    key2: &str,
    value2: &str,
    key3: &str,
    value3: &str,
    key4: &str,
    value4: &str,
) {
    let interests = [
        (key1.to_owned(), value1.to_owned()),
        (key2.to_owned(), value2.to_owned()),
        (key3.to_owned(), value3.to_owned()),
        (key4.to_owned(), value4.to_owned()),
    ];
    let no_map: Option<&str> = None;
    gps.request_map(&interests, no_map).await
}

#[wasm_bindgen]
pub fn load_gps_from_string(input: &str) -> Gps {
    console_error_panic_hook::set_once();
    let reader = std::io::Cursor::new(input);
    Gps::new(reader)
}

#[wasm_bindgen]
pub fn gps_from_area(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> Gps {
    Gps::from_area(vec![
        Node::new(xmin, ymin),
        Node::new(xmax, ymin),
        Node::new(xmax, ymax),
        Node::new(xmin, ymax),
    ])
}

pub fn load_gps_from_file(path: &str) -> std::io::Result<Gps> {
    let gpx_file = std::fs::File::open(path)?;
    let gpx_reader = std::io::BufReader::new(gpx_file);
    Ok(Gps::new(gpx_reader))
}

impl Gps {
    fn new<R: Read>(gpx_reader: R) -> Self {
        // load all points composing the trace and mark commented points
        // as special waypoints.
        let (mut waypoints, p) = parse_gpx_points(gpx_reader);

        // detect sharp turns before path simplification to keep them
        detect_sharp_turns(&p, &mut waypoints);
        waypoints.insert(p.first().copied().unwrap());
        waypoints.insert(p.last().copied().unwrap());
        println!("we have {} waypoints", waypoints.len());

        println!("initially we had {} points", p.len());

        // simplify path
        let rp = if p.len() < 100 {
            p.clone()
        } else {
            std::iter::successors(Some(0.00015), |precision| Some(precision / 2.))
                .map(|precision| {
                    // simplify path
                    let mut rp = Vec::new();
                    let mut segment = Vec::new();
                    for point in &p {
                        segment.push(*point);
                        if waypoints.contains(point) && segment.len() >= 2 {
                            let mut s = simplify_path(&segment, precision);
                            rp.append(&mut s);
                            segment = rp.pop().into_iter().collect();
                        }
                    }
                    rp.append(&mut segment);
                    rp
                })
                .find(|rp| rp.len() > 80)
                .unwrap()
        };
        println!("we now have {} points", rp.len());
        let map_polygon = crate::inflate_polyline(&rp, crate::SIDE * 2.); // two tiles on each side
        Gps {
            waypoints: Some(waypoints),
            path: Some(rp),
            map_polygon,
            map: None,
            interests: Vec::new(),
        }
    }
    pub fn from_area(area: Vec<Node>) -> Self {
        Gps {
            waypoints: None,
            path: None,
            map_polygon: area,
            map: None,
            interests: Vec::new(),
        }
    }
    pub async fn request_map<P: AsRef<std::path::Path>>(
        &mut self,
        key_values: &[(String, String)],
        map_name: Option<P>,
    ) {
        let (map, interests) = crate::request_map_from(&self.map_polygon, key_values, map_name)
            .await
            .expect("failed requesting map");
        self.map = Some(map);
        self.interests = interests;
        self.clip_map()
    }
    pub fn load_map<P: AsRef<std::path::Path>>(
        &mut self,
        map_name: P,
        key_values: &[(String, String)],
    ) -> std::io::Result<()> {
        crate::load_map_and_interests(&map_name, key_values).map(|(map, interests)| {
            self.map = Some(map);
            self.interests = interests;
            self.clip_map()
        })
    }
    pub fn save_svg<P: AsRef<std::path::Path>>(&self, svg_path: P) -> std::io::Result<()> {
        let interests_nodes = UniColorNodes(
            self.interests
                .iter()
                .map(|(_, n)| n)
                .cloned()
                .collect::<Vec<_>>(),
        );

        let map = self.map.as_ref().unwrap();
        if let Some(gpx_path) = &self.path {
            save_svg(
                svg_path,
                map.bounding_box(),
                [
                    map as SvgW,
                    (&gpx_path.as_slice()) as SvgW,
                    &interests_nodes as SvgW,
                ],
            )
        } else {
            save_svg(
                svg_path,
                map.bounding_box(),
                [map as SvgW, &interests_nodes as SvgW],
            )
        }
    }

    fn clip_map(&mut self) {
        let map = self.map.as_mut().unwrap();
        let tiles_wanted = if let Some(gpx_path) = &self.path {
            let path_map: Map = gpx_path.clone().into();
            path_map
                .non_empty_tiles()
                .map(|(x, y)| {
                    (
                        x + path_map.first_tile.0 - map.first_tile.0,
                        y + path_map.first_tile.1 - map.first_tile.1,
                    )
                })
                .flat_map(|(x, y)| {
                    (x.saturating_sub(1)..(x + 2))
                        .flat_map(move |nx| (y.saturating_sub(1)..(y + 2)).map(move |ny| (nx, ny)))
                })
                .collect::<HashSet<(usize, usize)>>()
        } else {
            let xmin = self.map_polygon[0].x;
            let ymin = self.map_polygon[0].y;
            let xmax = self.map_polygon[2].x;
            let ymax = self.map_polygon[2].y;
            let width = xmax - xmin;
            let height = ymax - ymin;

            let min_x_tile = (xmin / crate::SIDE).floor() as usize;
            let max_x_tile = ((xmin + width) / crate::SIDE).floor() as usize;
            let min_y_tile = (ymin / crate::SIDE).floor() as usize;
            let max_y_tile = ((ymin + height) / crate::SIDE).floor() as usize;
            (min_x_tile..=max_x_tile)
                .cartesian_product(min_y_tile..=max_y_tile)
                .map(|(x, y)| (x - map.first_tile.0, y - map.first_tile.1))
                .collect::<HashSet<_>>()
        };
        map.keep_tiles(&tiles_wanted);

        if let Some(waypoints) = &self.waypoints {
            self.interests
                .extend(std::iter::repeat(0).zip(waypoints.iter().copied()));
        }
    }

    pub fn write_gps<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let map = self.map.as_ref().unwrap();
        eprintln!("saving interests");
        save_tiled_interests(&self.interests, map.side, writer)?;
        if let Some(gpx_path) = &self.path {
            if let Some(waypoints) = &self.waypoints {
                eprintln!("saving the path");
                save_path(gpx_path, waypoints, writer)?;
                eprintln!("saving the pathtiles");
                let path: Map = gpx_path.clone().into();
                path.save_tiles(writer, &[255, 0, 0])?;
            }
        }
        eprintln!("saving the maptiles");
        map.save_tiles(writer, &[0, 0, 0])?;
        eprintln!("all is saved");

        Ok(())
    }
}
