class Map {
  constructor(filename) {
    let buffer = require("Storage").readArrayBuffer(filename);
    let file_size = buffer.length;
    let offset = 0;
    // header
    this.first_tile = Uint64Array(buffer, offset, 2);
    offset += 2 * 8;
    this.grid_size = Uint64Array(buffer, offset, 2);
    offset += 2 * 8;
    this.start_coordinates = Float64Array(buffer, offset, 2);
    offset += 2 * 8;
    let size_array = Float64Array(buffer, offset, 1);
    this.size = size_array[0];
    offset += 8;
    let binary_ways_len = Uint64Array(buffer, offset, 1);
    offset += 8;
    this.binary_ways = Uint8Array(buffer, offset, binary_ways_len);
    offset += binary_ways_len;
    this.tiles_sizes_prefix = Uint32Array(buffer, offset);
  }
  display(current_x, current_y, cos_direction, sin_direction, scale_factor) {
    let local_x = current_x - this.start_coordinates[0];
    let local_y = current_y - this.start_coordinates[1];
    let tile_x = Math.floor(local_x / self.side);
    let tile_y = Math.floor(local_y / self.side);
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
    console.log("starting tile", tile_x, tile_y);
    let center_x = g.getWidth() / 2;
    let center_y = g.getHeight() / 2;
    let tile_num = tile_x + tile_y * self.grid_size[0];
    let offset = this.tiles_sizes_prefix[tile_num];
    let upper_limit = this.binary_ways.length;
    if (tile_num + 1 < this.tiles_sizes_prefix.length) {
      upper_limit = this.tiles_sizes_prefix[tile_num + 1];
    }
    while (offset < upper_limit) {
      let way_length = this.binary_ways[offset];
      offset += 1;
      let x = (tile_x + this.binary_ways[offset] / 255) * this.side;
      let y = (tile_y + this.binary_ways[offset + 1] / 255) * this.side;
      let scaled_x = x - current_x * scale_factor;
      let scaled_y = y - current_y * scale_factor;
      let rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
      let rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
      let final_x = center_x - Math.round(rotated_x);
      let final_y = center_y + Math.round(rotated_y);
      offset += 2;
      for (let i = 0; i < way_length - 1; i++) {
        let x = (tile_x + this.binary_ways[offset] / 255) * this.side;
        let y = (tile_y + this.binary_ways[offset + 1] / 255) * this.side;
        let scaled_x = x - current_position.lon * scale_factor;
        let scaled_y = y - current_position.lat * scale_factor;
        let rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
        let rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
        let new_final_x = center_x - Math.round(rotated_x);
        let new_final_y = center_y + Math.round(rotated_y);
        offset += 2;
        g.drawLine(final_x, final_y, new_final_x, new_final_y);
        final_x = new_final_x;
        final_y = new_final_y;
      }
    }
  }
}

let map = Map("test.map");
map.display(5.79, 45.22, 1, 0, 30000);
