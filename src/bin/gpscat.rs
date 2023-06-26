use byteorder::{LittleEndian, ReadBytesExt};
use gps::Node;
use std::io::Read;

#[derive(Debug)]
struct Dimensions {
    first_tile: (usize, usize),
    grid_size: (usize, usize),
    start_coordinates: (f64, f64),
    side: f64,
}

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
}

#[derive(Default)]
struct Gps {
    maps: Vec<Map>,
    path: Option<Path>,
    interests: Option<Interests>,
}

impl Gps {
    fn new(filename: &str) -> std::io::Result<Self> {
        let mut gps: Gps = Default::default();
        let mut reader = std::io::BufReader::new(std::fs::File::open(filename)?);
        while let Ok(block_type) = reader.read_u8() {
            match block_type {
                0 => {
                    //tiles
                    let map = Map::new(&mut reader)?;
                    gps.maps.push(map);
                }
                1 => {
                    // streets
                    todo!()
                }
                2 => {
                    // path
                    gps.path = Some(Path::new(&mut reader)?);
                }
                3 => {
                    // interests
                    gps.interests = Some(Interests::new(&mut reader)?);
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
    fn new<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let type_size = reader.read_u8()?;
        let entry_size = reader.read_u8()? as usize;
        let non_empty_tiles_number = reader.read_u16::<LittleEndian>()? as usize;
        let mut non_empty_tiles = std::iter::repeat_with(|| -> std::io::Result<usize> {
            reader.read_u16::<LittleEndian>().map(|s| s as usize)
        })
        .take(non_empty_tiles_number)
        .collect::<Result<Vec<_>, _>>()?;
        let mut buffy = [0u8; 4];
        let non_empty_tiles_ends = std::iter::repeat_with(|| {
            if type_size == 16 {
                reader.read_u16::<LittleEndian>().map(|s| s as usize)
            } else {
                assert_eq!(type_size, 24);
                reader.read_exact(&mut buffy)?;
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
        let tiles_offsets = TilesOffsets::new(reader)?;
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
        let tiles_offsets = TilesOffsets::new(reader)?;
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

fn main() {
    if let Some(filename) = std::env::args().nth(1) {
        let gps = Gps::new(&filename).unwrap();
    } else {
        println!("give a filename");
    }
}
