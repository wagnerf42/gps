use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
};
use wasm_bindgen::prelude::*;

use crate::{
    detect_sharp_turns,
    gpx::{save_heights, save_path},
    parse_gpx_points, save_svg, save_tiled_interests, simplify_path,
    svg::{save_svg_to_writer, UniColorNodes},
    Map, Node, Svg, SvgW,
};

#[wasm_bindgen]
pub struct Gps {
    path: Option<Vec<Node>>,
    waypoints: Option<HashSet<Node>>,
    map_polygon: Vec<Node>,
    interests: Vec<(usize, Node)>,
    map: Option<Map>,
    heights: Option<HashMap<Node, f64>>,
    autodetect_waypoints: bool,
}

#[wasm_bindgen]
pub fn disable_elevation(gps: &mut Gps) {
    gps.heights = None;
}

#[wasm_bindgen]
pub fn get_gps_map_svg(gps: &Gps) -> String {
    let mut svg_string: Vec<u8> = Vec::new();
    let bounding_box = gps
        .map
        .as_ref()
        .map(|m| m.bounding_box())
        .unwrap_or_else(|| {
            gps.path
                .as_ref()
                .map(|p| {
                    let (xmin, xmax) = p.iter().map(|n| n.x).minmax().into_option().unwrap();
                    let (ymin, ymax) = p.iter().map(|n| n.y).minmax().into_option().unwrap();
                    (xmin, ymin, xmax, ymax)
                })
                .unwrap()
        });
    let path_slice = if let Some(p) = &gps.path {
        p.as_slice()
    } else {
        &[]
    };
    save_svg_to_writer(
        &mut svg_string,
        bounding_box,
        gps.map
            .as_ref()
            .map(|m| m as &dyn Svg<_>)
            .into_iter()
            .chain(std::iter::once(&path_slice as &dyn Svg<_>))
            .chain(std::iter::once(&UniColorNodes(
                gps.interests.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            ) as &dyn Svg<_>)),
        true,
    )
    .unwrap();
    String::from_utf8(svg_string).unwrap()
}

#[wasm_bindgen]
pub fn get_polygon(gps: &Gps) -> Vec<f64> {
    gps.map_polygon.iter().flat_map(|n| [n.y, n.x]).collect()
}

#[wasm_bindgen]
pub fn has_heights(gps: &Gps) -> bool {
    gps.heights.is_some()
}

#[wasm_bindgen]
pub fn get_polyline(gps: &Gps) -> Vec<f64> {
    gps.path
        .as_ref()
        .map(|p| p.iter().flat_map(|n| [n.y, n.x]).collect::<Vec<_>>())
        .unwrap_or_default()
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
pub fn load_gps_from_string(input: &str, autodetect_waypoints: bool) -> Gps {
    console_error_panic_hook::set_once();
    let reader = std::io::Cursor::new(input);
    Gps::new(reader, autodetect_waypoints)
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

pub fn load_gps_from_file(path: &str, autodetect_waypoints: bool) -> std::io::Result<Gps> {
    let gpx_file = std::fs::File::open(path)?;
    let gpx_reader = std::io::BufReader::new(gpx_file);
    Ok(Gps::new(gpx_reader, autodetect_waypoints))
}

impl Gps {
    fn new<R: Read>(gpx_reader: R, mut autodetect_waypoints: bool) -> Self {
        // load all points composing the trace and mark commented points
        // as special waypoints.
        let (mut waypoints, p, heights) = parse_gpx_points(gpx_reader);

        // brouter has a tendency to generate small loops
        // around its waypoints
        // we remove them here.
        let p = p
            .first()
            .cloned()
            .into_iter()
            .chain(p.windows(3).filter_map(|w| {
                if w.first() == w.last() && w[0].distance_to(&w[1]) < 0.00015 {
                    None
                } else {
                    Some(w[1])
                }
            }))
            .chain(p.last().cloned())
            .collect::<Vec<_>>();

        // detect sharp turns before path simplification to keep them
        // detect_sharp_turns(&p, &mut waypoints);
        waypoints.insert(p.first().copied().unwrap());
        waypoints.insert(p.last().copied().unwrap());

        if waypoints.len() > 2 {
            // if we have some manual waypoints, let's not detect
            autodetect_waypoints = false;
        }

        let rp = simplify_path_around_waypoints(&p, &waypoints);

        let (mut xmin, mut xmax) = rp.iter().map(|p| p.x).minmax().into_option().unwrap();
        let (mut ymin, mut ymax) = rp.iter().map(|p| p.y).minmax().into_option().unwrap();
        let map_polygon = if (xmax - xmin) * (ymax - ymin) < 0.2 * 0.2 {
            // osm should be able to answer this full rectangle
            xmin -= crate::SIDE * 2.;
            ymin -= crate::SIDE * 2.;
            xmax += crate::SIDE * 2.;
            ymax += crate::SIDE * 2.;
            vec![
                Node::new(xmin, ymin),
                Node::new(xmin, ymax),
                Node::new(xmax, ymax),
                Node::new(xmax, ymin),
            ]
        } else {
            inflate_polyline(&rp, crate::SIDE * 2.) // two tiles on each side
        };
        Gps {
            waypoints: Some(waypoints),
            path: Some(if autodetect_waypoints { p } else { rp }),
            map_polygon,
            map: None,
            interests: Vec::new(),
            heights: Some(heights),
            autodetect_waypoints,
        }
    }
    pub fn detect_crossroads(&mut self) {
        let (path, map, waypoints) = (&mut self.path, &self.map, &mut self.waypoints);
        if let Some(path) = path {
            if let Some(map) = map {
                if let Some(waypoints) = waypoints {
                    if waypoints.len() <= 2 {
                        // if we have two waypoints it's start and end
                        map.detect_crossroads(path, waypoints);
                    }
                }
            }
        }
    }
    pub fn from_area(area: Vec<Node>) -> Self {
        Gps {
            waypoints: None,
            path: None,
            map_polygon: area,
            map: None,
            interests: Vec::new(),
            heights: None,
            autodetect_waypoints: false,
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
        self.clip_map();
        if self.autodetect_waypoints {
            self.detect_crossroads();
        }
        self.add_waypoints_to_interests();
    }
    fn add_waypoints_to_interests(&mut self) {
        if let Some(waypoints) = &self.waypoints {
            self.interests
                .extend(std::iter::repeat(0).zip(waypoints.iter().copied()));
        }
    }
    pub fn load_map<P: AsRef<std::path::Path>>(
        &mut self,
        map_name: P,
        key_values: &[(String, String)],
    ) -> std::io::Result<()> {
        crate::load_map_and_interests(&map_name, key_values).map(|(map, interests)| {
            self.map = Some(map);
            self.interests = interests;
            self.clip_map();
            if self.autodetect_waypoints {
                self.detect_crossroads();
            }
            self.add_waypoints_to_interests();
        })
    }
    pub fn save_svg<P: AsRef<std::path::Path>>(&self, svg_path: P) -> std::io::Result<()> {
        let interests_nodes = UniColorNodes(
            self.interests
                .iter()
                .skip(1) // skip waypoints
                .map(|(_, n)| n)
                .cloned()
                .collect::<Vec<_>>(),
        );

        let waypoints_nodes =
            UniColorNodes(self.waypoints.iter().flatten().cloned().collect::<Vec<_>>());

        let map = self.map.as_ref().unwrap();
        if let Some(gpx_path) = &self.path {
            save_svg(
                svg_path,
                map.bounding_box(),
                [
                    map as SvgW,
                    (&gpx_path.as_slice()) as SvgW,
                    &interests_nodes as SvgW,
                    &waypoints_nodes as SvgW,
                ],
            )
        } else {
            save_svg(
                svg_path,
                map.bounding_box(),
                [
                    map as SvgW,
                    &interests_nodes as SvgW,
                    &waypoints_nodes as SvgW,
                ],
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
                        (x as isize + path_map.first_tile.0 - map.first_tile.0) as usize,
                        (y as isize + path_map.first_tile.1 - map.first_tile.1) as usize,
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

            let min_x_tile = (xmin / crate::SIDE).floor() as isize;
            let max_x_tile = ((xmin + width) / crate::SIDE).floor() as isize;
            let min_y_tile = (ymin / crate::SIDE).floor() as isize;
            let max_y_tile = ((ymin + height) / crate::SIDE).floor() as isize;
            (min_x_tile..=max_x_tile)
                .cartesian_product(min_y_tile..=max_y_tile)
                .map(|(x, y)| {
                    (
                        (x - map.first_tile.0) as usize,
                        (y - map.first_tile.1) as usize,
                    )
                })
                .collect::<HashSet<_>>()
        };
        map.keep_tiles(&tiles_wanted);
        self.interests.retain(|(_, p)| {
            let tile_x = ((p.x / crate::SIDE).floor() as isize - map.first_tile.0) as usize;
            let tile_y = ((p.y / crate::SIDE).floor() as isize - map.first_tile.1) as usize;
            tiles_wanted.contains(&(tile_x, tile_y))
        });
        map.fit_map();
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
            if let Some(heights) = &self.heights {
                save_heights(gpx_path, heights, writer)?;
            }
        }
        eprintln!("saving the maptiles");
        map.save_tiles(writer, &[0, 0, 0])?;
        eprintln!("all is saved");

        Ok(())
    }
}

fn inflate_polyline(rp: &[Node], side: f64) -> Vec<Node> {
    use geo_types::MultiPoint;
    let displaced_points: MultiPoint = rp
        .iter()
        .flat_map(|p| {
            (0..8).map(move |a| ((a as f64).cos() * side + p.x, (a as f64).sin() * side + p.y))
        })
        .collect::<Vec<(f64, f64)>>()
        .into();

    use geo::KNearestConcaveHull;
    let poly = displaced_points.0.k_nearest_concave_hull(10);
    poly.exterior()
        .points()
        .map(|p| Node::new(p.x(), p.y()))
        .collect()
}

pub fn simplify_path_around_waypoints(p: &Vec<Node>, waypoints: &HashSet<Node>) -> Vec<Node> {
    println!("we have {} waypoints", waypoints.len());

    println!("initially we had {} points", p.len());

    // simplify path
    let mut rp = Vec::new();
    let mut segment = Vec::new();
    for point in p {
        segment.push(*point);
        if waypoints.contains(point) && segment.len() >= 2 {
            let mut s = simplify_path(&segment, 0.00015);
            rp.append(&mut s);
            segment = rp.pop().into_iter().collect();
        }
    }
    rp.append(&mut segment);
    println!("we now have {} points", rp.len());
    rp
}
