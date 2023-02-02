class CWayId {
  constructor(tile_id, local_way_id) {
    this.tile_id = tile_id;
    this.local_way_id = local_way_id;
  }
}
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
    offset += this.tiles_sizes_prefix[this.tiles_sizes_prefix.length -1];
    
    // now do streets data header
    let streets_header = E.toArrayBuffer(s.read(filename, offset, 8));
    let streets_header_offset = 0;
    let full_streets_size = Uint32Array(streets_header, streets_header_offset, 1)[0];
    streets_header_offset += 4;
    let blocks_number = Uint16Array(streets_header, streets_header_offset, 1)[0];
    streets_header_offset += 2;
    let labels_string_size = Uint16Array(streets_header, streets_header_offset, 1)[0];
    streets_header_offset += 2;
    offset += streets_header_offset;
    
    // continue with main streets labels
    main_streets_labels = s.read(filename, offset, labels_string_size);
    // this.main_streets_labels = main_streets_labels.split(/\r?\n/);
    this.main_streets_labels = main_streets_labels.split(/\n/);
    offset += labels_string_size;
    
    // continue with blocks start offsets
    this.blocks_offsets = Uint32Array(E.toArrayBuffer(s.read(filename, offset, blocks_number *4)));
    offset += blocks_number * 4;
    
    // continue with compressed street blocks
    let encoded_blocks_size = full_streets_size - 4 - 2 - 2 - labels_string_size - blocks_number * 4; 
    this.compressed_streets = Uint8Array(E.toArrayBuffer(s.read(filename, offset, encoded_blocks_size)));
    offset += encoded_blocks_size;
  }
  display(current_x, current_y, cos_direction, sin_direction, scale_factor) {
    console.log("we are at", current_x, current_y);
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
    let center_x = g.getWidth() / 2;
    let center_y = g.getHeight() / 2;
    let tile_num = tile_x + tile_y * this.grid_size[0];

    let color_index = tile_num % 6;
    let colors = ["#f00", "#0f0", "#00f", "#ff0", "#f0f", "#0ff"];
    g.setColor(colors[color_index]);

    let line_start_offset = 0;
    if (tile_y > 0) {
      line_start_offset =
        this.tiles_sizes_prefix[tile_y * this.grid_size[0] - 1];
    }

    let offset = 0;
    if (tile_num >= 1) {
      offset = this.tiles_sizes_prefix[tile_num - 1] - line_start_offset;
    }
    let upper_limit = this.tiles_sizes_prefix[tile_num] - line_start_offset;
    while (offset < upper_limit) {
      let x = (tile_x + this.binary_lines[tile_y][offset] / 255) * this.side;
      let y =
        (tile_y + this.binary_lines[tile_y][offset + 1] / 255) * this.side;
      let scaled_x = (x - current_x) * scale_factor;
      let scaled_y = (y - current_y) * scale_factor;
      let rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
      let rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
      let final_x = center_x - Math.round(rotated_x);
      let final_y = center_y + Math.round(rotated_y);
      offset += 2;
      x = (tile_x + this.binary_lines[tile_y][offset] / 255) * this.side;
      y = (tile_y + this.binary_lines[tile_y][offset + 1] / 255) * this.side;
      scaled_x = (x - current_x) * scale_factor;
      scaled_y = (y - current_y) * scale_factor;
      rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
      rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
      let new_final_x = center_x - Math.round(rotated_x);
      let new_final_y = center_y + Math.round(rotated_y);
      offset += 2;
      g.drawLine(final_x, final_y, new_final_x, new_final_y);
    }
  }
  select_street() {
    function show_street_submenu(k) {
      map.select_street_block(k);
    }
    let main_menu = {};
    for(let i=0; i < this.main_streets_labels.length -1; i++) { // TODO: virer lignes vides dans rust
      let j = new Number(i);
      let label_copy = this.main_streets_labels[i].split('').join(''); // without this it does not work
      main_menu[label_copy] = function() {
        E.showMenu();
        show_street_submenu(j);
      };
    }
    E.showMenu(main_menu);
  }
  select_street_block(block_number) {
    
    let start = this.blocks_offsets[block_number];
    let end = this.blocks_offsets[block_number+1]; // TODO: fixme
    let compressed_block = this.compressed_streets.slice(start, end);
    let uncompressed_block = require('heatshrink').decompress(compressed_block);
    let ways_size = Uint16Array(uncompressed_block)[0];
    let raw_block = Uint8Array(uncompressed_block);
    let raw_ways_labels = raw_block.slice(2+ways_size, uncompressed_block.length);
    let raw_ways = raw_block.slice(2, 2+ways_size);
    let ways_labels = '';
    for(let i=0 ; i < raw_ways_labels.length ; i++) {
      ways_labels += String.fromCharCode(raw_ways_labels[i]);
    }
    labels = ways_labels.split(/\n/);
    
    let menu = {};
    for(let i=0; i < labels.length; i++) {
      let j = new Number(i);
      let label_copy = labels[i].split('').join(''); // without this it does not work
      menu[label_copy] = function() {
        let offset = 0;
        let way_length;
        for (let i=0; i<j; i++) {
          way_length = raw_ways[offset] + (raw_ways[offset+1] << 8);
          offset += 2 + 3 * way_length;
        }
        way_length = raw_ways[offset] + (raw_ways[offset+1] << 8);
        let street = [];
        offset += 2; // skip length
        for (let i=0; i<way_length; i++) {
          let tile_id = raw_ways[offset] + (raw_ways[offset+1] << 8);
          offset += 2;
          let local_way_id = raw_ways[offset];
          offset += 1;
          street.push(new CWayId(tile_id, local_way_id));
        }
        E.showMenu();
        map.go_to(street);
      };
    }
    E.showMenu(menu);
  }
  go_to(street) {
    console.log("going to", street);
  }
}

let map = new Map("test.map");
let x = 5.79;
let y = 45.22;
map.select_street();


  // map.display(x, y, 1, 0, 60000);
// setInterval(function() {
//   x+=1/10000;
//   y+=1/10000;
//   map.display(x, y, 1, 0, 60000);
  
// }, 1000);
console.log("DONE");
