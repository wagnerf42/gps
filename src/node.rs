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

impl Node {
    pub fn new(x: f64, y: f64) -> Self {
        Node { x, y }
    }
    pub fn squared_distance_between(&self, other: &Node) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dx * dx + dy * dy
    }
    pub fn distance_to_segment(&self, v: &Node, w: &Node) -> f64 {
        let l2 = v.squared_distance_between(w);
        if l2 == 0.0 {
            return self.squared_distance_between(v).sqrt();
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

        proj.squared_distance_between(self).sqrt()
    }

    // Loop on all tiles the node belongs.
    pub fn tiles(&self, side: f64) -> impl Iterator<Item = (usize, usize)> {
        let x = self.x / side;
        let y = self.y / side;
        let x_key = x.floor() as usize;
        let y_key = y.floor() as usize;
        let x_key_2 = x.ceil() as usize;
        let y_key_2 = y.ceil() as usize;
        let left = (x_key == x_key_2).then_some((x_key - 1, y_key));
        let top = (y_key == y_key_2).then_some((x_key, y_key - 1));
        let top_left = ((x_key == x_key_2) && (y_key == y_key_2)).then_some((x_key - 1, y_key - 1));
        std::iter::once((x_key, y_key))
            .chain(left)
            .chain(top)
            .chain(top_left)
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
}
