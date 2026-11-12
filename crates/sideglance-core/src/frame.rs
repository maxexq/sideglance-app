//! A 16x16 picture as RGB triples, row by row from the top left. Device-neutral.

pub const SIZE: usize = 16;

pub type Rgb = [u8; 3];
pub const BLACK: Rgb = [0, 0, 0];

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame(pub Vec<Rgb>);

impl Default for Frame {
    fn default() -> Self {
        Frame(vec![BLACK; SIZE * SIZE])
    }
}

impl Frame {
    pub fn get(&self, x: usize, y: usize) -> Rgb {
        self.0[y * SIZE + x]
    }

    pub fn set(&mut self, x: usize, y: usize, c: Rgb) {
        if x < SIZE && y < SIZE {
            self.0[y * SIZE + x] = c;
        }
    }

    /// "#rrggbb" per pixel, the form the web page draws.
    pub fn hex(&self) -> Vec<String> {
        self.0.iter().map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect()
    }

    pub fn from_hex(pixels: &[String]) -> Result<Frame, String> {
        let mut f = Frame::default();
        for (i, p) in pixels.iter().take(SIZE * SIZE).enumerate() {
            f.0[i] = parse_color(p)?;
        }
        Ok(f)
    }
}

/// "#ff8800" or "ff8800" to RGB.
pub fn parse_color(text: &str) -> Result<Rgb, String> {
    let t = text.trim_start_matches('#');
    if t.len() != 6 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("A color is 6 hex digits, for example ff8800 (got '{text}')"));
    }
    let n = u32::from_str_radix(t, 16).unwrap();
    Ok([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}
