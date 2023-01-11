class Map {
  constructor(filename) {
    console.log("starting", process.memory());

    let s = require("Storage");
    let buffer = s.readArrayBuffer(filename);
    let offset = 0;
    // header
    this.first_tile = Uint32Array(buffer, offset, 2);
    offset += 2 * 4;
    this.grid_size = Uint32Array(buffer, offset, 2);
    offset += 2 * 4;
    this.start_coordinates = Float64Array(buffer, offset, 2);
    offset += 2 * 8;
    let side_array = Float64Array(buffer, offset, 1);
    this.side = side_array[0];
    offset += 8;

    // tiles offsets
    let tiles_number = this.grid_size[0] * this.grid_size[1];
    let tiles_sizes_prefix_string = s.read(filename, offset, tiles_number * 3);
    let tiles_sizes_prefix_buffer = E.toArrayBuffer(tiles_sizes_prefix_string);
    this.tiles_sizes_prefix = Uint24Array(tiles_sizes_prefix_buffer);
    offset += 3 * tiles_number;

    // now, do binary ways
    // since the file is so big we'll go line by line
    let binary_lines = [];
    for (let y = 0; y < this.grid_size[1]; y++) {
      let first_tile_offset = 0;
      if (y > 0) {
        first_tile_offset = this.tiles_sizes_prefix[y * this.grid_size[0] - 1];
      }
      let last_tile_offset =
        this.tiles_sizes_prefix[y * this.grid_size[0] + this.grid_size[0] - 1];
      let size = last_tile_offset - first_tile_offset;
      let string = s.read(filename, offset + first_tile_offset, size);
      let array = Uint8Array(E.toArrayBuffer(string));
      binary_lines.push(array);
    }
    this.binary_lines = binary_lines;
  }
  display(current_x, current_y, cos_direction, sin_direction, scale_factor) {
    g.clear();
    let local_x = current_x - this.start_coordinates[0];
    let local_y = current_y - this.start_coordinates[1];
    let tile_x = Math.floor(local_x / this.side);
    let tile_y = Math.floor(local_y / this.side);
    for (let y = tile_y - 1; y <= tile_y + 1; y++) {
      if (y < 0 || y >= this.grid_size[1]) {
        continue;
      }
      for (let x = tile_x - 1; x <= tile_x + 1; x++) {
        if (x < 0 || x >= this.grid_size[0]) {
          continue;
        }
        this.display_tile(
          x,
          y,
          local_x,
          local_y,
          cos_direction,
          sin_direction,
          scale_factor
        );
      }
    }
  }
  display_tile(
    tile_x,
    tile_y,
    current_x,
    current_y,
    cos_direction,
    sin_direction,
    scale_factor
  ) {
    console.log("displaying tile at", tile_x, tile_y);
    let center_x = g.getWidth() / 2;
    let center_y = g.getHeight() / 2;
    let tile_num = tile_x + tile_y * this.grid_size[0];

    let line_start_offset = 0;
    if (tile_y > 0) {
      line_start_offset =
        this.tiles_sizes_prefix[tile_y * this.grid_size[0] - 1];
    }
    console.log("line starts at", line_start_offset);

    let offset = 0;
    if (tile_num >= 1) {
      offset = this.tiles_sizes_prefix[tile_num - 1] - line_start_offset;
    }
    let upper_limit = this.tiles_sizes_prefix[tile_num] - line_start_offset;
    while (offset < upper_limit) {
      let way_length = this.binary_lines[tile_y][offset];
      console.log("offset", offset, "way_length", way_length);
      offset += 1;
      let x = (tile_x + this.binary_lines[tile_y][offset] / 255) * this.side;
      let y =
        (tile_y + this.binary_lines[tile_y][offset + 1] / 255) * this.side;
      console.log(x, y);
      let scaled_x = (x - current_x) * scale_factor;
      let scaled_y = (y - current_y) * scale_factor;
      let rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
      let rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
      let final_x = center_x - Math.round(rotated_x);
      let final_y = center_y + Math.round(rotated_y);
      offset += 2;
      for (let i = 0; i < way_length - 1; i++) {
        let x = (tile_x + this.binary_lines[tile_y][offset] / 255) * this.side;
        let y =
          (tile_y + this.binary_lines[tile_y][offset + 1] / 255) * this.side;
        console.log("xy:", x, y);
        let scaled_x = (x - current_x) * scale_factor;
        let scaled_y = (y - current_y) * scale_factor;
        let rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
        let rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
        let new_final_x = center_x - Math.round(rotated_x);
        let new_final_y = center_y + Math.round(rotated_y);
        console.log("f:", new_final_x, new_final_y);
        offset += 2;
        g.drawLine(final_x, final_y, new_final_x, new_final_y);
        final_x = new_final_x;
        final_y = new_final_y;
      }
    }
  }
}

let map = new Map("test.map");
map.display(5.79, 45.22, 1, 0, 30000);
console.log("DONE");
