const TILE_BORDER_THICKNESS = 1.0 / 111200.0;
const SQUARED_TILE_BORDER_THICKNESS =
  TILE_BORDER_THICKNESS * TILE_BORDER_THICKNESS;

const STREET_SHOW = 1;
const STREET_GREEDY = 2;
const STREET_ASTAR = 3;

const SIMULATED = false;

// these variables form the global state of the system.
// they are not in a struct as to keep things fast.
let map = null;
let displaying = false; // disable display if we are already displaying
let displayed_x; // center point
let displayed_y;
let street_action = null; // what to do after selecting a street
let current_street = null; // street we selected
let tiled_street = null; // selected street in a display friendly format
let street_interval = null; // interval for street changes detection (to force freeing of memory at end of street menu)
let angle = 0; // where we look at
let cos_direction = 1; // cos and sin of angle
let sin_direction = 0;
let scale_factor = 60000; // map scale
let in_menu = false; // deactivate stroke/tap events when in menu
let position = null; // where we are
let tiled_path = null;

class HeapEntry {
  constructor(predecessor, travel_start, travel_end, distance) {
    this.predecessor = predecessor;
    this.travel_start = travel_start;
    this.travel_end = travel_end;
    this.distance = distance;
  }
}

function heappush(heap, entry) {
  heap.push(entry);
  if (heap.length == 1) {
    return;
  }

  // up we go
  let current_index = heap.length - 1;
  // 0 1 2 3 4 5 6 7 8  dad(5) = 2 dad(6) = 2 -> dad(i) = (i-1)/2
  let dad = Math.floor((current_index - 1) / 2);
  while (heap[dad].distance > heap[current_index].distance) {
    let tmp = heap[dad];
    heap[dad] = heap[current_index];
    heap[current_index] = tmp;
    current_index = dad;
    if (current_index == 0) {
      return;
    }
    dad = Math.floor((current_index - 1) / 2);
  }
}

function heappop(heap) {
  let min = heap[0];
  let last = heap.pop();
  if (heap.length > 0) {
    heap[0] = last;

    // down we go
    let current_index = 0;
    while (current_index * 2 + 1 < heap.length) {
      let smallest_son = current_index * 2 + 1;
      let maybe_other_son = current_index * 2 + 2;
      if (
        maybe_other_son < heap.length &&
        heap[smallest_son].distance > heap[maybe_other_son].distance
      ) {
        smallest_son = maybe_other_son;
      }
      if (heap[current_index].distance < heap[smallest_son].distance) {
        break;
      }
      let tmp = heap[current_index];
      heap[current_index] = heap[smallest_son];
      heap[smallest_son] = tmp;
      current_index = smallest_son;
    }
  }
  return min;
}

class Point {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }
  squared_distance_to(other) {
    let xdiff = this.x - other.x;
    let ydiff = this.y - other.y;
    return xdiff * xdiff + ydiff * ydiff;
  }
  distance(other_point) {
    //see https://www.movable-type.co.uk/scripts/latlong.html
    const R = 6371e3; // meters
    const phi1 = (this.y * Math.PI) / 180;
    const phi2 = (other_point.y * Math.PI) / 180;
    const deltaphi = ((other_point.y - this.y) * Math.PI) / 180;
    const deltalambda = ((other_point.x - this.x) * Math.PI) / 180;

    const a =
      Math.sin(deltaphi / 2) * Math.sin(deltaphi / 2) +
      Math.cos(phi1) *
        Math.cos(phi2) *
        Math.sin(deltalambda / 2) *
        Math.sin(deltalambda / 2);
    const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));

    return R * c; // in meters
  }
  // return all tiles we belong to (in absolute coordinates)
  tiles(side) {
    let x = Math.round((this.x * 255) / side);
    let y = Math.round((this.y * 255) / side);
    let x_key = Math.floor(x / 255);
    let y_key = Math.floor(y / 255);
    let right_tile_border = (x_key + 1) * side;
    let at_right = right_tile_border - this.x < TILE_BORDER_THICKNESS;
    let left_tile_border = x_key * side;
    let at_left = this.x - left_tile_border < TILE_BORDER_THICKNESS;
    let top_tile_border = (y_key + 1) * side;
    let at_top = top_tile_border - this.y < TILE_BORDER_THICKNESS;
    let bottom_tile_border = y_key * side;
    let at_bottom = this.y - bottom_tile_border < TILE_BORDER_THICKNESS;
    let tiles = [[x_key, y_key]];
    if (at_left) {
      tiles.push([x_key - 1, y_key]);
    }
    if (at_top && at_left) {
      tiles.push([x_key - 1, y_key + 1]);
    }
    if (at_top) {
      tiles.push([x_key, y_key + 1]);
    }
    if (at_top && at_right) {
      tiles.push([x_key + 1, y_key + 1]);
    }
    if (at_right) {
      tiles.push([x_key + 1, y_key]);
    }
    if (at_right && at_bottom) {
      tiles.push([x_key + 1, y_key - 1]);
    }
    if (at_bottom) {
      tiles.push([x_key, y_key - 1]);
    }
    if (at_bottom && at_left) {
      tiles.push([x_key - 1, y_key - 1]);
    }
    return tiles;
  }
}

class CNodeId {
  constructor(tile_number, local_node_id) {
    this.tile_number = tile_number;
    this.local_node_id = local_node_id;
  }
}

class GNode {
  constructor(id, node) {
    this.id = id;
    this.point = node;
  }
  is(other) {
    return (
      this.point.squared_distance_to(other.point) <=
      SQUARED_TILE_BORDER_THICKNESS
    );
  }
}

class CWayId {
  constructor(tile_number, local_way_id) {
    this.tile_number = tile_number;
    this.local_way_id = local_way_id;
  }
}
class Map {
  constructor(filename) {
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
    offset += this.tiles_sizes_prefix[this.tiles_sizes_prefix.length - 1];

    // now do streets data header
    let streets_header = E.toArrayBuffer(s.read(filename, offset, 8));
    let streets_header_offset = 0;
    let full_streets_size = Uint32Array(
      streets_header,
      streets_header_offset,
      1
    )[0];
    streets_header_offset += 4;
    let blocks_number = Uint16Array(
      streets_header,
      streets_header_offset,
      1
    )[0];
    streets_header_offset += 2;
    let labels_string_size = Uint16Array(
      streets_header,
      streets_header_offset,
      1
    )[0];
    streets_header_offset += 2;
    offset += streets_header_offset;

    // continue with main streets labels
    main_streets_labels = s.read(filename, offset, labels_string_size);
    // this.main_streets_labels = main_streets_labels.split(/\r?\n/);
    this.main_streets_labels = main_streets_labels.split(/\n/);
    offset += labels_string_size;

    // continue with blocks start offsets
    this.blocks_offsets = Uint32Array(
      E.toArrayBuffer(s.read(filename, offset, blocks_number * 4))
    );
    offset += blocks_number * 4;

    // continue with compressed street blocks
    let encoded_blocks_size =
      full_streets_size - 4 - 2 - 2 - labels_string_size - blocks_number * 4;
    this.compressed_streets = Uint8Array(
      E.toArrayBuffer(s.read(filename, offset, encoded_blocks_size))
    );
    offset += encoded_blocks_size;
  }
  display() {
    if (displaying || in_menu) {
      return; // don't draw on drawings
    }
    displaying = true;

    console.log("we are at", displayed_x, displayed_y);
    g.clear();
    let local_x = displayed_x - this.start_coordinates[0];
    let local_y = displayed_y - this.start_coordinates[1];
    let tile_x = Math.floor(local_x / this.side);
    let tile_y = Math.floor(local_y / this.side);
    let tiles_to_display = [];
    for (let y = tile_y - 1; y <= tile_y + 1; y++) {
      if (y < 0 || y >= this.grid_size[1]) {
        continue;
      }
      for (let x = tile_x - 1; x <= tile_x + 1; x++) {
        if (x < 0 || x >= this.grid_size[0]) {
          continue;
        }
        this.display_tile(x, y, local_x, local_y);
        tiles_to_display.push(x + y * this.grid_size[0]);
      }
    }
    if (tiled_street !== null) {
      this.display_tile_path(tiled_street, tiles_to_display);
    }
    if (tiled_path !== null) {
      this.display_tile_path(tiled_path, tiles_to_display);
    }
    if (position !== null) {
      let my_coordinates = this.point_screen_coordinates(
        position.x,
        position.y
      );
      g.setColor(0, 0, 0).fillCircle(my_coordinates[0], my_coordinates[1], 3);
    }
    displaying = false;
  }
  point_screen_coordinates(x, y) {
    let center_x = g.getWidth() / 2;
    let center_y = g.getHeight() / 2;
    let scaled_x = (x - displayed_x) * scale_factor;
    let scaled_y = (y - displayed_y) * scale_factor;
    let rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
    let rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
    let screen_x = center_x - Math.round(rotated_x);
    let screen_y = center_y + Math.round(rotated_y);
    return [screen_x, screen_y];
  }
  display_tile_path(path_tiles, tiles_to_display) {
    let next_tile_to_display = 0;
    let center_x = g.getWidth() / 2;
    let center_y = g.getHeight() / 2;
    g.setColor(1, 0, 0);
    for (let i = 0; i < path_tiles.length; i++) {
      let tile_number = path_tiles[i][0];
      while (tiles_to_display[next_tile_to_display] < tile_number) {
        next_tile_to_display++;
        if (next_tile_to_display >= tiles_to_display.length) {
          return;
        }
      }
      if (tile_number < tiles_to_display[next_tile_to_display]) {
        continue;
      }

      let tile_content = path_tiles[i][1];
      for (let j = 0; j < tile_content.length; j += 4) {
        let x1 = tile_content[j];
        let y1 = tile_content[j + 1];
        let x2 = tile_content[j + 2];
        let y2 = tile_content[j + 3];

        let scaled_x = (x1 - displayed_x) * scale_factor;
        let scaled_y = (y1 - displayed_y) * scale_factor;
        let rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
        let rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
        let final_x1 = center_x - Math.round(rotated_x);
        let final_y1 = center_y + Math.round(rotated_y);

        scaled_x = (x2 - displayed_x) * scale_factor;
        scaled_y = (y2 - displayed_y) * scale_factor;
        rotated_x = scaled_x * cos_direction - scaled_y * sin_direction;
        rotated_y = scaled_x * sin_direction + scaled_y * cos_direction;
        let final_x2 = center_x - Math.round(rotated_x);
        let final_y2 = center_y + Math.round(rotated_y);
        g.drawLine(final_x1, final_y1, final_x2, final_y2);
      }
    }
  }
  // turn the given street (array of CWayId) into an array (indexed by tile) of arrays of segments
  street_to_tiled_path(street) {
    // we need to weasel around to keep low memory

    // first, figure out which tiles are useful
    let street_tiles = [];
    for (let i = 0; i < street.length; i++) {
      street_tiles.push(street[i].tile_number);
    }
    street_tiles.sort();
    let unique_street_tiles = [street_tiles[0]];
    for (let i = 1; i < street_tiles.length; i++) {
      let tile_number = street_tiles[i];
      if (unique_street_tiles[unique_street_tiles.length - 1] != tile_number) {
        unique_street_tiles.push(tile_number);
      }
    }

    // now loop on the tiles and extract all ways
    let tiled_ways = [];

    for (let i = 0; i < unique_street_tiles.length; i++) {
      let tile_number = unique_street_tiles[i];
      let tile_x = tile_number % this.grid_size[0];
      let tile_y = (tile_number - tile_x) / this.grid_size[0];

      let line_start_offset = 0;
      if (tile_y > 0) {
        line_start_offset =
          this.tiles_sizes_prefix[tile_y * this.grid_size[0] - 1];
      }

      let tile_offset = 0;
      if (tile_number >= 1) {
        tile_offset =
          this.tiles_sizes_prefix[tile_number - 1] - line_start_offset;
      }

      let tile_ways = [];
      for (let i = 0; i < street.length; i++) {
        let way = street[i];
        if (way.tile_number != tile_number) {
          continue;
        }
        let offset = tile_offset + 4 * way.local_way_id;

        let x1 =
          (tile_x + this.binary_lines[tile_y][offset] / 255) * this.side +
          this.start_coordinates[0];
        let y1 =
          (tile_y + this.binary_lines[tile_y][offset + 1] / 255) * this.side +
          this.start_coordinates[1];
        let x2 =
          (tile_x + this.binary_lines[tile_y][offset + 2] / 255) * this.side +
          this.start_coordinates[0];
        let y2 =
          (tile_y + this.binary_lines[tile_y][offset + 3] / 255) * this.side +
          this.start_coordinates[1];
        tile_ways.push(x1);
        tile_ways.push(y1);
        tile_ways.push(x2);
        tile_ways.push(y2);
      }
      if (tile_ways.length > 0) {
        tiled_ways.push([tile_number, tile_ways]);
      }
    }
    return tiled_ways;
  }
  display_tile(tile_x, tile_y, current_x, current_y) {
    let center_x = g.getWidth() / 2;
    let center_y = g.getHeight() / 2;

    let tile_num = tile_x + tile_y * this.grid_size[0];
    // let color_index = tile_num % 6;
    // let colors = ["#f00", "#0f0", "#00f", "#ff0", "#f0f", "#0ff"];
    // g.setColor(colors[color_index]);
    g.setColor("#000");

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
    current_street = null;
    tiled_street = null;
    function show_street_submenu(k) {
      E.showMenu();
      map.select_street_block(k);
    }
    let main_menu = {};
    for (let i = 0; i < this.main_streets_labels.length - 1; i++) {
      // TODO: virer lignes vides dans rust
      let label_copy = this.main_streets_labels[i].split("").join(""); // without this it does not work
      main_menu[label_copy] = show_street_submenu.bind(null, i);
    }
    E.showMenu(main_menu);

    street_interval = setInterval(function () {
      if (current_street !== null) {
        clearInterval(street_interval);
        tiled_street = map.street_to_tiled_path(current_street);
        street_act();
      }
    }, 1000);
  }
  select_street_block(block_number) {
    let start = this.blocks_offsets[block_number];
    let end = this.blocks_offsets[block_number + 1]; // TODO: fixme
    let compressed_block = this.compressed_streets.slice(start, end);
    let uncompressed_block = require("heatshrink").decompress(compressed_block);
    let ways_size = Uint16Array(uncompressed_block)[0];
    let raw_block = Uint8Array(uncompressed_block);
    let raw_ways_labels = raw_block.slice(
      2 + ways_size,
      uncompressed_block.length
    );
    let raw_ways = raw_block.slice(2, 2 + ways_size);
    let ways_labels = "";
    for (let i = 0; i < raw_ways_labels.length; i++) {
      ways_labels += String.fromCharCode(raw_ways_labels[i]);
    }
    labels = ways_labels.split(/\n/);

    function extract_street(j) {
      let offset = 0;
      let way_length;
      for (let i = 0; i < j; i++) {
        way_length = raw_ways[offset] + (raw_ways[offset + 1] << 8);
        offset += 2 + 3 * way_length;
      }
      way_length = raw_ways[offset] + (raw_ways[offset + 1] << 8);
      let searched_street = [];
      offset += 2; // skip length
      for (let i = 0; i < way_length; i++) {
        let tile_id = raw_ways[offset] + (raw_ways[offset + 1] << 8);
        offset += 2;
        let local_way_id = raw_ways[offset];
        offset += 1;
        searched_street.push(new CWayId(tile_id, local_way_id));
      }
      E.showMenu();
      in_menu = false;
      current_street = searched_street; // propagate to global variable
    }

    let menu = {};
    for (let i = 0; i < labels.length; i++) {
      let label_copy = labels[i].split("").join(""); // without this it does not work
      menu[label_copy] = extract_street.bind(null, i);
    }
    E.showMenu(menu);
  }
  way(way_id) {
    let id1 = new CNodeId(way_id.tile_number, 2 * way_id.local_way_id);
    let id2 = new CNodeId(way_id.tile_number, 2 * way_id.local_way_id + 1);
    return [
      new GNode(id1, this.decode_node(id1)),
      new GNode(id2, this.decode_node(id2)),
    ];
  }
  decode_node(node_id) {
    let tile_x = node_id.tile_number % this.grid_size[0];
    let tile_y = Math.floor(node_id.tile_number / this.grid_size[0]);
    let tile_start = 0;
    if (node_id.tile_number > 0) {
      tile_start = this.tiles_sizes_prefix[node_id.tile_number - 1];
    }
    let first_tile_in_line = tile_y * this.grid_size[0];
    let line_start = 0;
    if (first_tile_in_line > 0) {
      line_start = this.tiles_sizes_prefix[first_tile_in_line - 1];
    }
    let binary_start = tile_start - line_start;

    let cx =
      this.binary_lines[tile_y][binary_start + 2 * node_id.local_node_id];
    let cy =
      this.binary_lines[tile_y][binary_start + 2 * node_id.local_node_id + 1];
    let x =
      this.start_coordinates[0] + tile_x * this.side + (cx * this.side) / 255;
    let y =
      this.start_coordinates[1] + tile_y * this.side + (cy * this.side) / 255;
    return new Point(x, y);
  }
  // return all tiles (local coordinates) this point belongs to.
  node_tiles(point) {
    let first_tile_x = this.first_tile[0];
    let first_tile_y = this.first_tile[1];
    return point.tiles(this.side).map(function (t) {
      return [t[0] - first_tile_x, t[1] - first_tile_y];
    });
  }
  find_starting_node(gps_start) {
    let tiles_containing_start = this.node_tiles(gps_start);
    if (tiles_containing_start.length == 0) {
      return null;
    }
    let starting_tile = tiles_containing_start[0]; // TODO: lots of fixme
    let edges = this.tile_edges(starting_tile[0], starting_tile[1]);
    let nearest_node = null;
    let nearest_distance = Infinity;
    for (let i = 0; i < edges.length; i++) {
      for (let j = 0; j < 2; j++) {
        let n = edges[i][j];
        let d = n.point.squared_distance_to(gps_start);
        if (d < nearest_distance) {
          nearest_distance = d;
          nearest_node = n;
        }
      }
    }
    return nearest_node;
  }

  find_ending_node(gps_start, street_ways) {
    let min_distance = Infinity;
    let nearest_node = null;
    for (let i = 0; i < street_ways.length; i++) {
      let way = street_ways[i];
      let nodes = this.way(way);
      for (let j = 0; j < 2; j++) {
        let d = nodes[j].point.squared_distance_to(gps_start);
        if (d < min_distance) {
          min_distance = d;
          nearest_node = nodes[j];
        }
      }
    }
    return nearest_node;
  }
  // return number of ways inside given tile
  tile_ways_number(tile_number) {
    let tile_start = 0;
    if (tile_number > 0) {
      tile_start = this.tiles_sizes_prefix[tile_number - 1];
    }
    let tile_end = this.tiles_sizes_prefix[tile_number];
    let tile_binary_size = tile_end - tile_start;
    return tile_binary_size / 4;
  }
  // return all edges in given tile
  tile_edges(tile_x, tile_y) {
    let tile_number = tile_x + tile_y * this.grid_size[0];
    let edges = [];
    for (let i = 0; i < this.tile_ways_number(tile_number); i++) {
      let way = this.way(new CWayId(tile_number, i));
      edges.push(way);
    }
    return edges;
  }
  node_offset_id(id) {
    let tile_offset = 0;
    if (id.tile_number > 0) {
      tile_offset = this.tiles_sizes_prefix[id.tile_number - 1];
    }
    let offset = tile_offset + 2 * id.local_node_id;
    return offset / 2;
  }
  a_star(start, end) {
    let greedy_path_length = this.greedy_path_length(start, end);
    let heap = [];
    let seen_nodes_size = Math.ceil(
      this.tiles_sizes_prefix[this.tiles_sizes_prefix.length - 1] / 16
    );
    let seen_nodes = new Uint8Array(seen_nodes_size); // TODO: is it zeroed ?
    let predecessors = [];
    let entry = new HeapEntry(null, start, start, 0);
    while (entry != null) {
      let end_id = this.node_offset_id(entry.travel_end.id);
      if ((seen_nodes[Math.floor(end_id / 8)] & (1 << end_id % 8)) != 0) {
        entry = heappop(heap);
        continue;
      }
      let start_id = this.node_offset_id(entry.travel_start.id);
      seen_nodes[Math.floor(start_id / 8)] =
        seen_nodes[Math.floor(start_id / 8)] | (1 << start_id % 8);
      seen_nodes[Math.floor(end_id / 8)] =
        seen_nodes[Math.floor(end_id / 8)] | (1 << end_id % 8);
      let current_node = entry.travel_end;
      if (entry.predecessor !== null) {
        predecessors.push([current_node, entry.predecessor]);
      }
      if (current_node.is(end)) {
        return rebuild_path(current_node, predecessors);
      }
      let neighbours = this.neighbours(current_node);
      for (let i = 0; i < neighbours.length; i++) {
        let travel = neighbours[i];
        let d =
          entry.distance +
          Math.sqrt(travel[0].point.squared_distance_to(travel[1].point));
        if (
          d + Math.sqrt(travel[1].point.squared_distance_to(end.point)) <
          greedy_path_length
        ) {
          let entry = new HeapEntry(current_node, travel[0], travel[1], d);
          heappush(heap, entry);
        }
      }
      entry = heappop(heap);
    }
    return null;
  }
  greedy_path(start, end) {
    let predecessors = this.greedy_predecessors(start, end);
    if (predecessors !== null) {
      return rebuild_path(end, predecessors);
    } else {
      return null;
    }
  }
  greedy_path_length(start, end) {
    let predecessors = this.greedy_predecessors(start, end);
    if (predecessors !== null) {
      return rebuild_path_length(end, predecessors);
    } else {
      return null;
    }
  }
  greedy_predecessors(start, end) {
    let heap = [];
    let seen_nodes_size = Math.ceil(
      this.tiles_sizes_prefix[this.tiles_sizes_prefix.length - 1] / 16
    );
    let seen_nodes = new Uint8Array(seen_nodes_size); // TODO: is it zeroed ?
    let predecessors = [];
    let entry = new HeapEntry(
      null,
      start,
      start,
      start.point.squared_distance_to(end.point)
    );
    while (entry != null) {
      let end_id = this.node_offset_id(entry.travel_end.id);
      if ((seen_nodes[Math.floor(end_id / 8)] & (1 << end_id % 8)) != 0) {
        entry = heappop(heap);
        continue;
      }
      let start_id = this.node_offset_id(entry.travel_start.id);
      seen_nodes[Math.floor(start_id / 8)] =
        seen_nodes[Math.floor(start_id / 8)] | (1 << start_id % 8);
      seen_nodes[Math.floor(end_id / 8)] =
        seen_nodes[Math.floor(end_id / 8)] | (1 << end_id % 8);
      let current_node = entry.travel_end;
      if (entry.predecessor !== null) {
        predecessors.push([current_node, entry.predecessor]);
      }
      if (current_node.is(end)) {
        return predecessors;
      }
      let neighbours = this.neighbours(current_node);
      for (let i = 0; i < neighbours.length; i++) {
        let travel = neighbours[i];
        let entry = new HeapEntry(
          current_node,
          travel[0],
          travel[1],
          travel[1].point.squared_distance_to(end.point)
        );
        heappush(heap, entry);
      }
      entry = heappop(heap);
    }
    return null;
  }
  neighbours(node) {
    let tiles = this.node_tiles(node.point);
    let edges = [];
    for (let i = 0; i < tiles.length; i++) {
      let new_edges = this.tile_edges(tiles[i][0], tiles[i][1]);
      for (let j = 0; j < new_edges.length; j++) {
        let nodes = new_edges[j];
        if (nodes[0].is(node)) {
          edges.push(nodes);
        } else if (nodes[1].is(node)) {
          edges.push([nodes[1], nodes[0]]);
        }
      }
    }
    return edges;
  }
}

function rebuild_path(end, predecessors) {
  let path = [end];
  for (let i = predecessors.length - 1; i >= 0; i--) {
    let current_node = path[path.length - 1];
    let p = predecessors[i];
    if (p[0].is(current_node)) {
      //TODO: seems fishy
      path.push(p[1]);
    }
  }
  return path;
}

function rebuild_path_length(end, predecessors) {
  let current_node = end;
  let distance = 0;
  for (let i = predecessors.length - 1; i >= 0; i--) {
    let p = predecessors[i];
    if (p[0].is(current_node)) {
      distance += Math.sqrt(p[1].point.squared_distance_to(current_node.point));
      current_node = p[1];
    }
  }
  return distance;
}

function load_map(gps_file) {
  console.log("loading", gps_file);
  map = new Map(gps_file);
  displayed_x = map.start_coordinates[0] + (map.grid_size[0] * map.side) / 2;
  displayed_y = map.start_coordinates[1] + (map.grid_size[1] * map.side) / 2;
  E.showMenu();
  map.display();
}

let files = require("Storage").list(".gps");
if (files.length <= 1) {
  if (files.length == 0) {
    E.showAlert("no .gps file found").then(function () {
      load();
    });
  }
  load_map(files[0]);
} else {
  const menu = {
    "": { title: "choose map" },
  };
  for (let i = 0; i < files.length; i++) {
    menu[files[i]] = load_map.bind(null, files[i]);
  }
  E.showMenu(menu);
}

function street_act() {
  if (street_action == STREET_SHOW) {
    displayed_x = tiled_street[0][1][0];
    displayed_y = tiled_street[0][1][1];
    map.display();
  } else {
    if (position === null) {
      E.showAlert("wait first for gps signal").then(function () {
        map.display();
      });
    } else {
      let starting_node = map.find_starting_node(position);
      let ending_node = map.find_ending_node(position, current_street);
      let path;
      if (street_action == STREET_GREEDY) {
        path = map.greedy_path(starting_node, ending_node);
      } else {
        path = map.astar(starting_node, ending_node);
      }
      tiled_path = map.street_to_tiled_path(path);
    }
  }
}

Bangle.on("stroke", (o) => {
  if (in_menu || map === null) {
    return;
  }
  // we move display according to stroke
  let first_x = o.xy[0];
  let first_y = o.xy[1];
  let last_x = o.xy[o.xy.length - 2];
  let last_y = o.xy[o.xy.length - 1];
  let xdiff = last_x - first_x;
  let ydiff = last_y - first_y;

  let angle_back = -angle;
  let c = Math.cos((angle_back * Math.PI) / 180);
  let s = Math.sin((angle_back * Math.PI) / 180);
  let rotated_x = xdiff * c - ydiff * s;
  let rotated_y = xdiff * s + ydiff * c;
  displayed_x += rotated_x / ((scale_factor * 4) / 3);
  displayed_y -= rotated_y / ((scale_factor * 4) / 3);
  map.display();
});

Bangle.on("tap", function (tap) {
  if (in_menu || !tap.double) {
    return;
  }
  in_menu = true;
  const menu = {
    "": { title: "choose action" },
    "show street": function () {
      street_action = STREET_SHOW;
      map.select_street();
    },
    "greedy path": function () {
      street_action = STREET_GREEDY;
      map.select_street();
    },
    "a* path": function () {
      street_action = STREET_ASTAR;
      map.select_street();
    },
    "back to map": function () {
      in_menu = false;
      E.showMenu();
      map.display();
    },
  };
  E.showMenu(menu);
});

old_points = [];
function gps_coordinates(data) {
  // 0,0 coordinates are considered invalid since we sometimes receive them out of nowhere
  let valid_coordinates =
    !isNaN(data.lat) &&
    !isNaN(data.lon) &&
    (data.lat != 0.0 || data.lon != 0.0);
  if (valid_coordinates) {
    let new_position = new Point(data.lon, data.lat);

    if (old_points.length == 0) {
      old_points.push(new_position);
    } else {
      if (old_points.length == 4) {
        old_points.shift();
      }

      let previous_point = old_points[old_points.length - 1];
      let distance_to_previous = previous_point.distance(new_position);
      // gps signal is noisy but rarely above 4 meters
      if (distance_to_previous < 4) {
        return;
      }
      old_points.push(new_position);

      // let's just take angle of segment between newest point and a point a bit before
      let previous_index = old_points.length - 3;
      if (previous_index < 0) {
        previous_index = 0;
      }
      let xdiff = new_position.x - old_points[previous_index].x;
      let ydiff = new_position.y - old_points[previous_index].y;
      angle = Math.atan2(ydiff, xdiff);
      cos_direction = Math.cos(angle);
      sin_direction = Math.sin(angle);
    }
    position = new_position;
    displayed_x = position.x;
    displayed_y = position.y;
    map.display();
  }
}

if (!SIMULATED) {
  Bangle.setGPSPower(true, "planis");
  Bangle.on("GPS", gps_coordinates);
} else {
  console.log("welcome to the simulation");
  let i = 0.5;
  let going_forward = true;
  setInterval(function () {
    if (going_forward) {
      i += 1 / 5000;
    } else {
      i -= 1 / 5000;
    }
    if (map !== null) {
      let xmin = map.start_coordinates[0];
      let ymin = map.start_coordinates[1];
      let x = xmin + i * map.grid_size[0] * map.side;
      let y = ymin + i * map.grid_size[1] * map.side;
      gps_coordinates({ lon: x, lat: y });
    }
  }, 1000);
}
