use byteorder::{LittleEndian, ReadBytesExt};
use gps::{save_svg, Node, Svg, SvgW};
use itertools::Itertools;
use std::io::Read;

#[derive(Debug)]
struct Dimensions {
    first_tile: (usize, usize),
    grid_size: (usize, usize),
    start_coordinates: (f64, f64),
    side: f64,
}

const COLORS: [&str; 5] = ["yellow", "red", "blue", "cyan", "green"];

impl Dimensions {
    fn new<R: Read>(reader: &mut R) -> std::io::Result<Dimensions> {
        let first_tile = (
            reader.read_u32::<LittleEndian>()? as usize,
            reader.read_u32::<LittleEndian>()? as usize,
        );

        let grid_size = (
            reader.read_u32::<LittleEndian>()? as usize,
            reader.read_u32::<LittleEndian>()? as usize,
        );

        let start_coordinates = (
            reader.read_f64::<LittleEndian>()?,
            reader.read_f64::<LittleEndian>()?,
        );
        let side = reader.read_f64::<LittleEndian>()?;
        Ok(Dimensions {
            first_tile,
            grid_size,
            start_coordinates,
            side,
        })
    }
    fn bounding_box(&self) -> (f64, f64, f64, f64) {
        let xmin = self.first_tile.0 as f64 * self.side;
        let ymin = self.first_tile.1 as f64 * self.side;
        let xmax = xmin + self.side * self.grid_size.0 as f64;
        let ymax = ymin + self.side * self.grid_size.1 as f64;
        (xmin, ymin, xmax, ymax)
    }
}

#[derive(Default)]
struct Gps {
    maps: Vec<Map>,
    path: Option<Path>,
    interests: Option<Interests>,
    heights: Option<Vec<i16>>,
}

impl Gps {
    fn new(filename: &str) -> std::io::Result<Self> {
        let mut gps: Gps = Default::default();
        let mut reader = std::io::BufReader::new(std::fs::File::open(filename)?);
        while let Ok(block_type) = reader.read_u8() {
            match block_type {
                0 => {
                    //tiles
                    eprintln!("parsing map");
                    let map = Map::new(&mut reader)?;
                    eprintln!("done parsing map");
                    gps.maps.push(map);
                }
                1 => {
                    // streets
                    todo!()
                }
                2 => {
                    // path
                    eprintln!("parsing path");
                    gps.path = Some(Path::new(&mut reader)?);
                    eprintln!("done parsing path");
                }
                3 => {
                    // interests
                    eprintln!("we have some interests");
                    gps.interests = Some(Interests::new(&mut reader)?);
                    eprintln!("done parsing interests");
                }
                4 => {
                    // heights
                    eprintln!("parsing heights");
                    let path_length = gps
                        .path
                        .as_ref()
                        .map(|p| p.points.len())
                        .unwrap_or_default();
                    let mut heights = Vec::new();
                    for _ in 0..path_length {
                        heights.push(reader.read_i16::<LittleEndian>()?);
                    }
                    gps.heights = Some(heights);
                }
                _ => panic!("invalid block type {block_type}"),
            }
        }
        Ok(gps)
    }
}

#[derive(Debug)]
struct TilesOffsets {
    entry_size: usize,
    non_empty_tiles: Vec<usize>,
    non_empty_tiles_ends: Vec<usize>,
}

impl TilesOffsets {
    fn new<R: Read>(reader: &mut R, dimensions: &Dimensions) -> std::io::Result<Self> {
        let type_size = reader.read_u8()?;
        let entry_size = reader.read_u8()? as usize;
        let non_empty_tiles_number = reader.read_u16::<LittleEndian>()? as usize;
        let bytes_per_tile_index =
            if dimensions.grid_size.0 * dimensions.grid_size.1 <= std::u16::MAX as usize {
                2
            } else {
                3
            };

        let mut buffy = [0u8; 4];
        let non_empty_tiles = std::iter::repeat_with(|| -> std::io::Result<usize> {
            if bytes_per_tile_index == 2 {
                reader.read_u16::<LittleEndian>().map(|s| s as usize)
            } else {
                reader.read_exact(&mut buffy[0..3])?;
                buffy[3] = 0;
                let mut rdr = std::io::Cursor::new(buffy);
                rdr.read_u32::<LittleEndian>().map(|s| s as usize)
            }
        })
        .take(non_empty_tiles_number)
        .collect::<Result<Vec<_>, _>>()?;

        let non_empty_tiles_ends = std::iter::repeat_with(|| {
            if type_size == 16 {
                reader.read_u16::<LittleEndian>().map(|s| s as usize)
            } else {
                assert_eq!(type_size, 24);
                reader.read_exact(&mut buffy[0..3])?;
                buffy[3] = 0;
                let mut rdr = std::io::Cursor::new(buffy);
                rdr.read_u32::<LittleEndian>().map(|s| s as usize)
            }
        })
        .take(non_empty_tiles_number)
        .collect::<Result<Vec<_>, _>>()?;
        Ok(TilesOffsets {
            entry_size,
            non_empty_tiles,
            non_empty_tiles_ends,
        })
    }

    fn end_offset(&self) -> usize {
        self.non_empty_tiles_ends.last().unwrap() * self.entry_size
    }
}

#[derive(Debug)]
struct Interests {
    dimensions: Dimensions,
    tiles_offsets: TilesOffsets,
    binary_interests: Vec<u8>,
}

impl Interests {
    fn new<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let dimensions = Dimensions::new(reader)?;
        let tiles_offsets = TilesOffsets::new(reader, &dimensions)?;
        let end = tiles_offsets.end_offset();
        let mut binary_interests = vec![0u8; end];
        reader.read_exact(&mut binary_interests)?;
        Ok(Interests {
            dimensions,
            tiles_offsets,
            binary_interests,
        })
    }
}

#[derive(Debug)]
struct Path {
    points: Vec<Node>,
    waypoints: Vec<u8>,
}

impl Path {
    fn new<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let points_number = reader.read_u16::<LittleEndian>()? as usize;
        let mut points = Vec::with_capacity(points_number);
        for _ in 0..points_number {
            let x = reader.read_f64::<LittleEndian>()?;
            let y = reader.read_f64::<LittleEndian>()?;
            points.push(Node::new(x, y));
        }
        let waypoints_length = (points_number >> 3) + if points_number % 8 == 0 { 0 } else { 1 };
        let mut waypoints = Vec::new();
        for _ in 0..waypoints_length {
            waypoints.push(reader.read_u8()?);
        }

        Ok(Path { points, waypoints })
    }
}

#[derive(Debug)]
struct Map {
    color_array: [u8; 3],
    dimensions: Dimensions,
    tiles_offsets: TilesOffsets,
    binary: Vec<u8>,
}

impl Map {
    fn new<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let red = reader.read_u8()?;
        let green = reader.read_u8()?;
        let blue = reader.read_u8()?;
        let color_array = [red, green, blue];
        let dimensions = Dimensions::new(reader)?;
        let tiles_offsets = TilesOffsets::new(reader, &dimensions)?;
        let end = tiles_offsets.end_offset();
        let mut binary = vec![0; end];
        reader.read_exact(&mut binary)?;
        Ok(Map {
            color_array,
            dimensions,
            tiles_offsets,
            binary,
        })
    }
}
impl<W: std::io::Write> Svg<W> for Interests {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        let ends = self
            .tiles_offsets
            .non_empty_tiles_ends
            .iter()
            .map(|o| o * self.tiles_offsets.entry_size);
        let starts = std::iter::once(0).chain(ends.clone());
        let tiles = self.tiles_offsets.non_empty_tiles.iter();
        let width = self.dimensions.grid_size.0;
        let side = self.dimensions.side;

        for (tile, (start, end)) in tiles.zip(starts.zip(ends)) {
            let tile_x = tile % width;
            let tile_y = tile / width;
            let absolute_tile_x = self.dimensions.first_tile.0 + tile_x;
            let absolute_tile_y = self.dimensions.first_tile.1 + tile_y;
            for (interest, x, y) in self.binary_interests[start..end].iter().tuples() {
                let x = ((*x as f64) / 255. + absolute_tile_x as f64) * side;
                let y = ((*y as f64) / 255. + absolute_tile_y as f64) * side;
                // eprintln!("<line x1='{x1}' y1='{y1}' ({start_x}/{start_y}) x2='{x2}' y2='{y2}' ({end_x}/{end_y})/>");
                writeln!(
                    writer,
                    "<circle cx='{x}' cy='{y}' r='0.1%' fill='{}'/>",
                    COLORS[*interest as usize]
                )?;
            }
        }
        Ok(())
    }
}

impl<W: std::io::Write> Svg<W> for Map {
    fn write_svg(&self, writer: &mut W, color: &str) -> std::io::Result<()> {
        let ends = self
            .tiles_offsets
            .non_empty_tiles_ends
            .iter()
            .map(|o| o * self.tiles_offsets.entry_size);
        let starts = std::iter::once(0).chain(ends.clone());
        let tiles = self.tiles_offsets.non_empty_tiles.iter();
        let width = self.dimensions.grid_size.0;
        let side = self.dimensions.side;
        let [red, green, blue] = self.color_array;
        writeln!(
            writer,
            "<g stroke='rgb({red}, {green}, {blue})' stroke-width='0.1%'>"
        )?;
        for (tile, (start, end)) in tiles.zip(starts.zip(ends)) {
            let tile_x = tile % width;
            let tile_y = tile / width;
            let absolute_tile_x = self.dimensions.first_tile.0 + tile_x;
            let absolute_tile_y = self.dimensions.first_tile.1 + tile_y;
            // eprintln!("tile {tile} ({tile_x}/{tile_y})  ({absolute_tile_x}/{absolute_tile_y})starts at {start} and ends at {end}");
            for (start_x, start_y, end_x, end_y) in self.binary[start..end].iter().tuples() {
                let x1 = ((*start_x as f64) / 255. + absolute_tile_x as f64) * side;
                let x2 = ((*end_x as f64) / 255. + absolute_tile_x as f64) * side;
                let y1 = ((*start_y as f64) / 255. + absolute_tile_y as f64) * side;
                let y2 = ((*end_y as f64) / 255. + absolute_tile_y as f64) * side;
                // eprintln!("<line x1='{x1}' y1='{y1}' ({start_x}/{start_y}) x2='{x2}' y2='{y2}' ({end_x}/{end_y})/>");
                writeln!(writer, "<line x1='{x1}' y1='{y1}' x2='{x2}' y2='{y2}'/>")?;
            }
        }
        writeln!(writer, "</g>")?;
        Ok(())
    }
}

fn main() {
    if let Some(filename) = std::env::args().nth(1) {
        let gps = Gps::new(&filename).unwrap();

        if let Some(path) = gps.path.as_ref() {
            path.points.iter().enumerate().for_each(|(i, p)| {
                if path.waypoints[i / 8] & (1 << (i % 8)) != 0 {
                    eprint!("*** ");
                }
                eprintln!(
                    "point num {i} : {p:?} (height: {})",
                    gps.heights
                        .as_ref()
                        .map(|h| h[i])
                        .clone()
                        .unwrap_or_default()
                )
            });
        }

        let bbox = gps.maps.last().unwrap().dimensions.bounding_box();
        save_svg(
            "debug.svg",
            bbox,
            gps.maps
                .iter()
                .rev()
                .map(|m| m as SvgW)
                .chain(gps.interests.as_ref().into_iter().map(|i| i as SvgW)),
        )
        .unwrap();
        std::process::Command::new("kitty")
            .arg("+kitten")
            .arg("icat")
            .arg("debug.svg")
            .status()
            .expect("running kitty failed");
    } else {
        println!("give a filename");
    }
}
