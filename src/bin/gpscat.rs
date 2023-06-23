use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Read;

#[derive(Debug)]
struct Dimensions {
    first_tile: (usize, usize),
    grid_size: (usize, usize),
    start_coordinates: (f64, f64),
    side: f64,
}

struct Layer {
    color: (u8, u8, u8),
    dimensions: Dimensions,
    tiles_offsets: Vec<usize>,
    binary_lines: Vec<Vec<u8>>,
}

struct Gps {
    layers: Vec<Layer>,
}

#[derive(Debug)]
struct TilesOffsets {
    entry_size: u8,
    non_empty_tiles: Vec<usize>,
    non_empty_tiles_ends: Vec<usize>,
}

fn parse_tiles_offsets<R: Read>(reader: &mut R) -> std::io::Result<TilesOffsets> {
    let type_size = reader.read_u8()?;
    let entry_size = reader.read_u8()?;
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

fn parse_dimensions<R: Read>(reader: &mut R) -> std::io::Result<Dimensions> {
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

fn load(filename: &str) -> std::io::Result<Gps> {
    let mut layers = Vec::new();
    let mut reader = std::io::BufReader::new(std::fs::File::open(filename)?);
    loop {
        let block_type = reader.read_u8()?;
        match block_type {
            0 => {
                //tiles
                let red = reader.read_u8()?;
                let green = reader.read_u8()?;
                let blue = reader.read_u8()?;
                println!("rgb {red} {green} {blue}");
            }
            1 => {
                // streets
                todo!()
            }
            2 => {
                // path
                todo!()
            }
            3 => {
                // interests
                let dimensions = parse_dimensions(&mut reader)?;
                eprintln!("dimensions {dimensions:?}");
                let tiles_offsets = parse_tiles_offsets(&mut reader)?;
                eprintln!("tiles offsets {tiles_offsets:?}");
                todo!()
            }
            _ => panic!("invalid block type"),
        }
        todo!()
    }
    Ok(Gps { layers })
}

fn main() {
    if let Some(filename) = std::env::args().nth(1) {
        let gps = load(&filename).unwrap();
    } else {
        println!("give a filename");
    }
}
