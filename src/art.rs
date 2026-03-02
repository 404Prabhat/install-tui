use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct ArtFrame {
    pub title: String,
    pub lines: Vec<String>,
    pub palette: usize,
}

pub struct MatrixArt {
    drops: Vec<i32>,
    trails: Vec<i32>,
    seed: u64,
    last_step: Instant,
    last_theme: Instant,
    palette: usize,
    banner: usize,
}

const CHARSET: &[u8] = b"01abcdefXYZ$#@*+=-<>[]{}";
const BANNERS: &[&str] = &[
    "Matrix Rain",
    "Neon Stream",
    "Cyber Installer",
    "Arch Velocity",
    "Quantum Packages",
];

impl MatrixArt {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        Self {
            drops: Vec::new(),
            trails: Vec::new(),
            seed,
            last_step: Instant::now(),
            last_theme: Instant::now(),
            palette: 0,
            banner: 0,
        }
    }

    pub fn frame(&mut self, width: u16, height: u16) -> ArtFrame {
        let w = width.max(4) as usize;
        let h = height.max(4) as usize;
        self.ensure_size(w, h);

        if self.last_theme.elapsed() >= Duration::from_secs(10) {
            self.palette = (self.palette + 1) % 5;
            self.banner = (self.banner + 1) % BANNERS.len();
            self.last_theme = Instant::now();
        }

        if self.last_step.elapsed() >= Duration::from_millis(120) {
            self.advance(h as i32);
            self.last_step = Instant::now();
        }

        let mut lines = Vec::with_capacity(h);
        for y in 0..h {
            let mut line = String::with_capacity(w);
            for x in 0..w {
                let drop = self.drops[x];
                let trail = self.trails[x];
                let yi = y as i32;

                if yi == drop {
                    line.push(self.rand_char());
                } else if yi < drop && yi >= drop - trail {
                    if (self.rand_u32() % 100) < 75 {
                        line.push(self.rand_char());
                    } else {
                        line.push(' ');
                    }
                } else {
                    line.push(' ');
                }
            }
            lines.push(line);
        }

        ArtFrame {
            title: BANNERS[self.banner].to_string(),
            lines,
            palette: self.palette,
        }
    }

    fn ensure_size(&mut self, width: usize, height: usize) {
        if self.drops.len() == width {
            return;
        }

        self.drops.clear();
        self.trails.clear();
        for _ in 0..width {
            let start = -((self.rand_u32() % (height as u32 + 1)) as i32);
            let trail = 4 + (self.rand_u32() % 10) as i32;
            self.drops.push(start);
            self.trails.push(trail);
        }
    }

    fn advance(&mut self, height: i32) {
        for i in 0..self.drops.len() {
            self.drops[i] += 1;
            if self.drops[i] - self.trails[i] > height {
                self.drops[i] = -((self.rand_u32() % (height as u32 + 1)) as i32);
                self.trails[i] = 4 + (self.rand_u32() % 10) as i32;
            }
        }
    }

    fn rand_char(&mut self) -> char {
        let idx = (self.rand_u32() as usize) % CHARSET.len();
        CHARSET[idx] as char
    }

    fn rand_u32(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.seed >> 32) as u32
    }
}
