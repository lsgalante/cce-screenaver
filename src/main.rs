use wayland_client::QueueHandle;
use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::widget::{MouseButton, ElementState, MouseScrollDelta, KeyEvent, TextItem};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ScreensaverStyle {
    Blank,
    Starfield,
    Matrix,
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_f32(&mut self) -> f32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let val = (self.state >> 32) as u32;
        (val as f32) / (u32::MAX as f32)
    }

    fn next_range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

struct Star {
    x: f32, // NDC coordinates: -1.0 to 1.0
    y: f32, // NDC coordinates: -1.0 to 1.0
    z: f32, // Distance: 0.1 to 1.0
    speed: f32,
    color: [f32; 4],
}

struct MatrixColumn {
    x: f32,     // X coordinate in pixels
    y: f32,     // Current falling Y coordinate of the head
    speed: f32, // Pixels per second
    len: usize, // Tail length (number of cells)
}

struct ScreensaverApp {
    style: ScreensaverStyle,
    width: u32,
    height: u32,
    scale_factor: f64,
    lcg: Lcg,
    stars: Vec<Star>,
    matrix_columns: Vec<MatrixColumn>,
    init_cursor: Option<LogicalPosition>,
    grace_timer: f32, // 1 second grace period to prevent instant exit on startup mouse wiggle
    text_items: Vec<TextItem>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum AppMessage {
    Exit,
}

impl Application for ScreensaverApp {
    type Message = AppMessage;

    fn new(_qh: &QueueHandle<EngineState<Self>>, _sender: calloop::channel::Sender<Self::Message>) -> Self {
        let style_env = std::env::var("CCE_SCREENSAVER_STYLE").unwrap_or_default();
        let style = match style_env.to_lowercase().as_str() {
            "starfield" => ScreensaverStyle::Starfield,
            "matrix" => ScreensaverStyle::Matrix,
            _ => {
                // Also parse command line args
                let args: Vec<String> = std::env::args().collect();
                if args.iter().any(|arg| arg == "starfield") {
                    ScreensaverStyle::Starfield
                } else if args.iter().any(|arg| arg == "matrix") {
                    ScreensaverStyle::Matrix
                } else {
                    ScreensaverStyle::Blank
                }
            }
        };

        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        
        let mut lcg = Lcg::new(seed);
        
        // Initialize stars
        let mut stars = Vec::with_capacity(200);
        for _ in 0..200 {
            stars.push(Star {
                x: lcg.next_range(-1.0, 1.0),
                y: lcg.next_range(-1.0, 1.0),
                z: lcg.next_range(0.1, 1.0),
                speed: lcg.next_range(0.15, 0.4),
                color: [
                    lcg.next_range(0.8, 1.0),
                    lcg.next_range(0.8, 1.0),
                    1.0,
                    1.0,
                ],
            });
        }

        Self {
            style,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
            lcg,
            stars,
            matrix_columns: Vec::new(),
            init_cursor: None,
            grace_timer: 1.0,
            text_items: Vec::new(),
        }
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "CCE Screensaver".to_string(),
            app_id: "cce-screenaver".to_string(),
            width: 1920,
            height: 1080,
            fullscreen: true,
            min_size: None,
        }
    }

    fn update(&mut self, msg: Self::Message, _needs_rebuild: &mut bool, exit: &mut bool) {
        match msg {
            AppMessage::Exit => {
                *exit = true;
            }
        }
    }

    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool) {
        if self.grace_timer > 0.0 {
            self.grace_timer -= dt;
        }

        match self.style {
            ScreensaverStyle::Starfield => {
                for star in &mut self.stars {
                    star.z -= star.speed * dt;
                    if star.z <= 0.0 {
                        star.z = 1.0;
                        star.x = self.lcg.next_range(-1.0, 1.0);
                        star.y = self.lcg.next_range(-1.0, 1.0);
                        star.speed = self.lcg.next_range(0.15, 0.4);
                    }
                }
                *needs_rebuild = true;
            }
            ScreensaverStyle::Matrix => {
                // Initialize columns if screen size changes or empty
                let cell_w = 20.0f32;
                let needed_cols = (self.width as f32 / cell_w).ceil() as usize;
                
                if self.matrix_columns.len() < needed_cols {
                    let old_len = self.matrix_columns.len();
                    for i in old_len..needed_cols {
                        self.matrix_columns.push(MatrixColumn {
                            x: i as f32 * cell_w,
                            y: self.lcg.next_range(-600.0, 0.0),
                            speed: self.lcg.next_range(120.0, 320.0),
                            len: self.lcg.next_range(8.0, 24.0) as usize,
                        });
                    }
                }

                for col in &mut self.matrix_columns {
                    col.y += col.speed * dt;
                    let cell_h = 24.0f32;
                    let total_h = col.len as f32 * cell_h;
                    if col.y - total_h > self.height as f32 {
                        col.y = -cell_h;
                        col.speed = self.lcg.next_range(120.0, 320.0);
                        col.len = self.lcg.next_range(8.0, 24.0) as usize;
                    }
                }
                *needs_rebuild = true;
            }
            ScreensaverStyle::Blank => {}
        }
    }

    fn view(&mut self, quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, size: LogicalSize, scale: f64) {
        self.width = size.width as u32;
        self.height = size.height as u32;
        self.scale_factor = scale;

        // Clear screen background
        quads.push((0.0, 0.0, self.width as f32, self.height as f32, [0.0, 0.0, 0.0, 1.0]));

        match self.style {
            ScreensaverStyle::Starfield => {
                let half_w = self.width as f32 / 2.0;
                let half_h = self.height as f32 / 2.0;
                let max_dim = half_w.max(half_h);

                for star in &self.stars {
                    // Project NDC style coordinates to screen space
                    let px = half_w + (star.x / star.z) * max_dim;
                    let py = half_h + (star.y / star.z) * max_dim;

                    // Only draw if inside screen bounds
                    if px >= 0.0 && px < self.width as f32 && py >= 0.0 && py < self.height as f32 {
                        // Star grows larger as it gets closer
                        let size = (2.0 / star.z).clamp(1.0, 8.0);
                        
                        // Fade in stars as they move out from the dark center
                        let alpha = ((1.0 - star.z) * 1.5).clamp(0.0, 1.0);
                        let mut color = star.color;
                        color[3] = alpha;

                        quads.push((px - size / 2.0, py - size / 2.0, size, size, color));
                    }
                }
            }
            ScreensaverStyle::Matrix => {
                let cell_w = 16.0f32;
                let cell_h = 20.0f32;

                for col in &self.matrix_columns {
                    // Draw each cell in the tail
                    for i in 0..col.len {
                        let cell_y = col.y - (i as f32 * cell_h);
                        if cell_y >= 0.0 && cell_y < self.height as f32 {
                            // Head cell is bright white/green, trailing cells fade to dark green
                            let color = if i == 0 {
                                [0.85, 1.0, 0.85, 1.0]
                            } else {
                                let fade = 1.0 - (i as f32 / col.len as f32);
                                [0.0, 0.7 * fade, 0.0, fade]
                            };
                            quads.push((col.x + 2.0, cell_y, cell_w - 4.0, cell_h - 4.0, color));
                        }
                    }
                }
            }
            ScreensaverStyle::Blank => {}
        }
    }

    fn text_items(&self) -> &[TextItem] {
        &self.text_items
    }

    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool) {
        if self.grace_timer > 0.0 {
            // Keep track of first position during grace period to measure movement distance
            if self.init_cursor.is_none() {
                self.init_cursor = Some(pos);
            }
            return;
        }

        if let Some(init) = self.init_cursor {
            let dx = (pos.x - init.x).abs();
            let dy = (pos.y - init.y).abs();
            if dx > 10.0 || dy > 10.0 {
                // Moved significantly after grace period, trigger exit
                *needs_rebuild = true;
                std::process::exit(0);
            }
        } else {
            *needs_rebuild = true;
            std::process::exit(0);
        }
    }

    fn handle_mouse_input(&mut self, _button: MouseButton, state: ElementState, _pos: LogicalPosition, _needs_rebuild: &mut bool) -> Option<Self::Message> {
        if state == ElementState::Pressed {
            std::process::exit(0);
        }
        None
    }

    fn handle_mouse_wheel(&mut self, _delta: &MouseScrollDelta, _pos: LogicalPosition, _needs_rebuild: &mut bool) {
        std::process::exit(0);
    }

    fn handle_key_input(&mut self, event: &KeyEvent, _needs_rebuild: &mut bool) -> Option<Self::Message> {
        if event.state == ElementState::Pressed {
            std::process::exit(0);
        }
        None
    }
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    cce_ui::engine::run::<ScreensaverApp>();
}
