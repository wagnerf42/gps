use std::{collections::HashMap, io::Write};

use itertools::Itertools;

use crate::{map::BlockType, Node};

pub fn save_tiled_interests<W: Write>(
    interests: &[(usize, Node)],
    side: f64,
    writer: &mut W,
) -> std::io::Result<()> {
    if interests.is_empty() {
        return Ok(());
    }
    let mut tiled_interests: HashMap<usize, Vec<(usize, Node)>> = HashMap::new();
    let (xmin, xmax) = interests
        .iter()
        .map(|(_, node)| node.x)
        .minmax()
        .into_option()
        .unwrap();

    let (ymin, ymax) = interests
        .iter()
        .map(|(_, node)| node.y)
        .minmax()
        .into_option()
        .unwrap();

    let first_tile_x = (xmin / side).floor() as isize;
    let first_tile_y = (ymin / side).floor() as isize;
    let grid_width = 1isize.max((xmax / side).floor() as isize - first_tile_x) as usize;
    let grid_height = 1isize.max((ymax / side).floor() as isize - first_tile_y) as usize;
    let xmin = (xmin / side).floor() * side;
    let ymin = (ymin / side).floor() * side;

    for (tile, interest) in interests.iter().map(|(interest_type, interest_node)| {
        (
            interest_node
                .tiles(side)
                .map(|(tx, ty)| ((tx - first_tile_x) as usize, (ty - first_tile_y) as usize))
                .map(|(tx, ty)| tx + ty * grid_width)
                .next() // first tile is enough for interests
                .unwrap(),
            (*interest_type, *interest_node),
        )
    }) {
        tiled_interests.entry(tile).or_default().push(interest);
    }
    let mut non_empty_tiles = tiled_interests.keys().copied().collect::<Vec<usize>>();
    non_empty_tiles.sort_unstable();

    writer.write_all(&[BlockType::Interests as u8])?;

    writer.write_all(&(first_tile_x as u32).to_le_bytes())?;
    writer.write_all(&(first_tile_y as u32).to_le_bytes())?;
    writer.write_all(&(grid_width as u32).to_le_bytes())?;
    writer.write_all(&(grid_height as u32).to_le_bytes())?;
    writer.write_all(&xmin.to_le_bytes())?;
    writer.write_all(&ymin.to_le_bytes())?;
    writer.write_all(&side.to_le_bytes())?;

    //TODO: factorize with save_sizes_prefix
    writer.write_all(&[16])?;
    writer.write_all(&[3])?; // size taken by each interest
    writer.write_all(&(non_empty_tiles.len() as u16).to_le_bytes())?;

    let bytes_per_tile_index = if grid_width * grid_height > std::u16::MAX as usize {
        3
    } else {
        2
    };
    for tile in &non_empty_tiles {
        writer.write_all(&tile.to_le_bytes()[0..bytes_per_tile_index])?;
    }

    for end in non_empty_tiles.iter().scan(0u16, |previous_end, tile_id| {
        let tile_size = tiled_interests[tile_id].len() as u16;
        *previous_end += tile_size;
        Some(*previous_end)
    }) {
        writer.write_all(&end.to_le_bytes())?;
    }
    for tile in &non_empty_tiles {
        for (interest_type, interest_node) in &tiled_interests[tile] {
            writer.write_all(&[*interest_type as u8])?;
            let tile_x = first_tile_x + (*tile as usize % grid_width) as isize;
            let tile_y = first_tile_y + (*tile as usize / grid_width) as isize;
            let encoded = interest_node.encode(tile_x, tile_y, side);
            // eprintln!("{interest_node:?} encodes as {encoded:?}, tile is {tile_x}/{tile_y}");
            writer.write_all(&encoded)?;
        }
    }

    Ok(())
}
