#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub x: f64,
    pub y: f64,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits() && self.y.to_bits() == other.y.to_bits()
    }
}
impl Eq for Node {}
impl std::hash::Hash for Node {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
    }
}

const TILE_BORDER_THICKNESS: f64 = 1. / 111_200.;

impl Node {
    pub fn new(x: f64, y: f64) -> Self {
        Node { x, y }
    }
    pub fn squared_distance_to(&self, other: &Node) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dx * dx + dy * dy
    }

    pub fn distance_to(&self, other: &Node) -> f64 {
        self.squared_distance_to(other).sqrt()
    }
    pub fn distance_to_segment(&self, v: &Node, w: &Node) -> f64 {
        let l2 = v.squared_distance_to(w);
        if l2 == 0.0 {
            return self.squared_distance_to(v).sqrt();
        }
        // Consider the line extending the segment, parameterized as v + t (w - v).
        // We find projection of point p onto the line.
        // It falls where t = [(p-v) . (w-v)] / |w-v|^2
        // We clamp t from [0,1] to handle points outside the segment vw.
        let x0 = self.x - v.x;
        let y0 = self.y - v.y;
        let x1 = w.x - v.x;
        let y1 = w.y - v.y;
        let dot = x0 * x1 + y0 * y1;
        let t = (dot / l2).clamp(0.0, 1.0);

        let proj = Node {
            x: v.x + x1 * t,
            y: v.y + y1 * t,
        };

        proj.distance_to(self)
    }

    // Loop on all tiles the node belongs.
    pub fn tiles(&self, side: f64) -> impl Iterator<Item = (usize, usize)> {
        let x = (self.x * 255. / side).round() as usize;
        let y = (self.y * 255. / side).round() as usize;
        let x_key = x / 255;
        let y_key = y / 255;

        let right_tile_border = (x_key + 1) as f64 * side;
        let at_right = right_tile_border - self.x < TILE_BORDER_THICKNESS;
        let left_tile_border = x_key as f64 * side;
        let at_left = self.x - left_tile_border < TILE_BORDER_THICKNESS;
        let top_tile_border = (y_key + 1) as f64 * side;
        let at_top = top_tile_border - self.y < TILE_BORDER_THICKNESS;
        let bottom_tile_border = y_key as f64 * side;
        let at_bottom = self.y - bottom_tile_border < TILE_BORDER_THICKNESS;

        let left = at_left.then_some((x_key - 1, y_key));
        let top_left = (at_top && at_left).then_some((x_key - 1, y_key + 1));
        let top = at_top.then_some((x_key, y_key + 1));
        let top_right = (at_top && at_right).then_some((x_key + 1, y_key + 1));
        let right = at_right.then_some((x_key + 1, y_key));
        let bottom_right = (at_bottom && at_right).then_some((x_key + 1, y_key - 1));
        let bottom = at_bottom.then_some((x_key, y_key - 1));
        let bottom_left = (at_bottom && at_left).then_some((x_key - 1, y_key - 1));
        [
            Some((x_key, y_key)),
            left,
            top_left,
            top,
            top_right,
            right,
            bottom_right,
            bottom,
            bottom_left,
        ]
        .into_iter()
        .flatten()
    }

    pub fn horizontal_segment_intersection(&self, n2: &Node, y: f64) -> Node {
        let fraction_of_segment = (y - self.y) / (n2.y - self.y);
        let x = self.x + fraction_of_segment * (n2.x - self.x);
        Node::new(x, y)
    }

    pub fn vertical_segment_intersection(&self, n2: &Node, x: f64) -> Node {
        let fraction_of_segment = (x - self.x) / (n2.x - self.x);
        let y = self.y + fraction_of_segment * (n2.y - self.y);
        Node::new(x, y)
    }

    pub(crate) fn encode(&self, x: usize, y: usize, side: f64) -> [u8; 2] {
        let x_offset = self.x - x as f64 * side;
        let y_offset = self.y - y as f64 * side;
        [
            (x_offset * 255. / side).round() as u8,
            (y_offset * 255. / side).round() as u8,
        ]
    }

    pub(crate) fn is(&self, other: &Node) -> bool {
        self.distance_to(other) <= TILE_BORDER_THICKNESS
    }
}
