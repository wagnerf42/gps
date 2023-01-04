// loop on all coordinates c intersecting grid at min + side * alpha
// such that start < c < end
pub fn grid_coordinates_between(
    mut start: f64,
    mut end: f64,
    side: f64,
) -> impl Iterator<Item = f64> {
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    let start_cell = start / side;
    let above_start_cell = start_cell.ceil();
    let real_start_cell = if start_cell == above_start_cell {
        (above_start_cell + 1.) as u32
    } else {
        above_start_cell as u32
    };
    let end_cell = (end / side).ceil() as u32;
    (real_start_cell..end_cell).map(move |alpha| alpha as f64 * side)
}
