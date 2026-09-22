//! Parameterized pixel-art character generator.
//!
//! Modes:
//!   cargo run -p character-gen                                  # regenerate default footballer
//!   cargo run -p character-gen -- --gallery /tmp/gallery.png    # 30-look contact sheet
//!   cargo run -p character-gen -- --pack DIR --skin N --hair N --kit NAME [--glasses] [--beard]
//!
//! All art is ORIGINAL pixel art — kit colors + number 10 only, no
//! club/brand logos, no real-person likeness data.

use image::{ImageBuffer, Rgba, RgbaImage};

const GRID: u32 = 48;
const SCALE: u32 = 3; // 144x144 output

type Px = Rgba<u8>;

const fn px(r: u8, g: u8, b: u8) -> Px {
    Rgba([r, g, b, 255])
}
const TRANSPARENT: Px = Rgba([0, 0, 0, 0]);

const OUTLINE: Px = Rgba([43, 34, 38, 255]);

// ---------------------------------------------------------------- palettes

type Skin = (Px, Px); // (base, shade)

const SKINS: [Skin; 5] = [
    (px(247, 220, 195), px(229, 193, 164)), // 1 fair
    (px(240, 200, 162), px(219, 172, 130)), // 2 light golden
    (px(216, 166, 120), px(192, 141, 96)),  // 3 tan
    (px(166, 113, 76), px(140, 91, 58)),    // 4 brown
    (px(113, 74, 48), px(92, 58, 36)),      // 5 deep
];

#[derive(Clone, Copy, PartialEq)]
enum HairStyle {
    Short,
    Curly,
    Quiff,
    Wavy,
}

struct HairDef {
    style: HairStyle,
    color: Px,
    shade: Px,
}

const HAIRS: [HairDef; 5] = [
    HairDef { style: HairStyle::Short, color: px(38, 32, 30), shade: px(24, 20, 19) }, // 1 black short
    HairDef { style: HairStyle::Short, color: px(92, 62, 38), shade: px(70, 46, 27) }, // 2 brown short
    HairDef { style: HairStyle::Curly, color: px(30, 26, 24), shade: px(20, 17, 16) }, // 3 black curly
    HairDef { style: HairStyle::Quiff, color: px(70, 48, 30), shade: px(52, 34, 21) }, // 4 brown quiff
    HairDef { style: HairStyle::Wavy, color: px(28, 24, 23), shade: px(18, 16, 15) },  // 5 black wavy/medium
];

#[derive(Clone, Copy, PartialEq)]
enum Kit {
    Argentina,
    Miami,
    Teal,
}

struct KitDef {
    body: Px,
    stripe: Px,
    trim: Px,
    shorts: Px,
    socks: Px,
    number: Px,
    boot: Px,
}

const KITS: [(Kit, &str, KitDef); 3] = [
    (
        Kit::Argentina,
        "argentina",
        KitDef {
            body: px(244, 246, 250),
            stripe: px(116, 172, 226),
            trim: px(230, 232, 238),
            shorts: px(38, 40, 48),
            socks: px(240, 242, 248),
            number: px(32, 56, 130),
            boot: px(232, 100, 44),
        },
    ),
    (
        Kit::Miami,
        "miami",
        KitDef {
            body: px(244, 160, 197),
            stripe: px(231, 136, 178),
            trim: px(255, 255, 255),
            shorts: px(32, 32, 38),
            socks: px(244, 160, 197),
            number: px(32, 32, 38),
            boot: px(250, 250, 252),
        },
    ),
    (
        Kit::Teal,
        "teal",
        KitDef {
            body: px(46, 168, 160),
            stripe: px(38, 144, 137),
            trim: px(255, 255, 255),
            shorts: px(240, 240, 244),
            socks: px(46, 168, 160),
            number: px(255, 255, 255),
            boot: px(232, 100, 44),
        },
    ),
];

fn kit_def(kit: Kit) -> &'static KitDef {
    &KITS.iter().find(|(k, _, _)| *k == kit).unwrap().2
}

struct Look {
    skin: usize,
    hair: usize,
    kit: Kit,
    glasses: bool,
    beard: bool,
}

// ---------------------------------------------------------------- canvas

struct Canvas {
    img: RgbaImage,
}

impl Canvas {
    fn new() -> Canvas {
        Canvas {
            img: ImageBuffer::new(GRID, GRID),
        }
    }
    fn px(&mut self, x: i32, y: i32, c: Px) {
        if x < 0 || y < 0 || x >= GRID as i32 || y >= GRID as i32 {
            return;
        }
        self.img.put_pixel(x as u32, y as u32, c);
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Px) {
        for dy in 0..h {
            for dx in 0..w {
                self.px(x + dx, y + dy, c);
            }
        }
    }
}

fn upscale(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = RgbaImage::new(w * SCALE, h * SCALE);
    for (x, y, p) in img.enumerate_pixels() {
        for dy in 0..SCALE {
            for dx in 0..SCALE {
                out.put_pixel(x * SCALE + dx, y * SCALE + dy, *p);
            }
        }
    }
    out
}

fn mirror(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.put_pixel(w - 1 - x, y, *img.get_pixel(x, y));
        }
    }
    out
}

// ---------------------------------------------------------------- drawing

/// Grid 48x48. Character faces RIGHT. `f` = frame 0..6 (walk cycle).
fn draw_look(look: &Look, f: u32, walking: bool) -> RgbaImage {
    let mut c = Canvas::new();
    let (skin, skin_shade) = SKINS[look.skin];
    let hair = &HAIRS[look.hair];
    let kit = kit_def(look.kit);

    let cx = 20i32; // body center
    let cycle = if walking {
        (f as f32 / 6.0) * std::f32::consts::TAU
    } else {
        0.0
    };
    let bob = (cycle.sin() * 1.0).round() as i32;
    let leg = cycle.sin();
    let lift = cycle.cos().abs().round() as i32;

    // ---- ground shadow ----
    for dy in 0..2i32 {
        for dx in -10..=10i32 {
            if dx.abs() <= 10 - dy * 4 {
                c.px(cx + dx, 45 + dy, Rgba([30, 30, 40, 70]));
            }
        }
    }

    // ---- legs ----
    let hip_y = 33 + bob;
    let l_leg = (leg * 3.0).round() as i32;
    let r_leg = (-leg * 3.0).round() as i32;
    let l_lift = if leg > 0.0 { lift } else { 0 };
    let r_lift = if leg < 0.0 { lift } else { 0 };
    draw_leg(&mut c, cx - 2 + l_leg, hip_y, l_lift, skin, kit);
    draw_leg(&mut c, cx + 2 + r_leg, hip_y, r_lift, skin_shade, kit);

    // ---- shorts ----
    c.rect(cx - 5, 30 + bob, 11, 4, kit.shorts);
    c.px(cx - 5, 30 + bob, OUTLINE);
    c.px(cx + 5, 30 + bob, OUTLINE);
    c.rect(cx - 5, 33 + bob, 11, 1, OUTLINE);

    // ---- jersey body (with vertical stripes) ----
    let body_y = 21 + bob;
    for row in 0..9 {
        for col in -5..=5 {
            let color = if (col + 5) % 4 == 0 || (col + 5) % 4 == 1 {
                kit.stripe
            } else {
                kit.body
            };
            c.px(cx + col, body_y + row, color);
        }
    }
    // trim collar + hem
    c.rect(cx - 5, body_y, 11, 1, kit.trim);
    c.rect(cx - 5, body_y + 8, 11, 1, kit.trim);
    // number 10 on chest (facing right, slightly right of center)
    draw_number10(&mut c, cx, body_y + 2, kit.number);
    // outline
    for y in 0..9 {
        c.px(cx - 6, body_y + y, OUTLINE);
        c.px(cx + 6, body_y + y, OUTLINE);
    }
    c.rect(cx - 6, body_y, 13, 1, OUTLINE);
    c.rect(cx - 6, body_y + 8, 13, 1, OUTLINE);

    // ---- arms (swing opposite to legs), short sleeves ----
    draw_arm(&mut c, cx - 7, body_y + 1, -leg, skin, skin_shade, kit);
    draw_arm(&mut c, cx + 7, body_y + 1, leg, skin, skin_shade, kit);

    // ---- head ----
    let head_y = 9 + bob; // top of hair
    draw_head(&mut c, cx, head_y, look, skin, skin_shade, hair);

    // ---- ball ----
    let ball_x = cx + 11 + (cycle.sin() * 1.5).round() as i32;
    let bounce = (cycle.sin().abs() * 2.0).round() as i32;
    draw_ball(&mut c, ball_x, 42 - bounce, f);

    upscale(&c.img)
}

fn draw_leg(c: &mut Canvas, x: i32, hip_y: i32, lift: i32, skin: Px, kit: &KitDef) {
    let foot_y = 43 - lift;
    c.rect(x, hip_y, 2, 4, skin); // thigh
    c.rect(x, foot_y - 4, 2, 4, kit.socks); // sock
    c.rect(x - 1, foot_y, 4, 2, kit.boot); // boot
    c.rect(x - 1, foot_y + 1, 4, 1, kit.boot);
}

fn draw_arm(
    c: &mut Canvas,
    x: i32,
    y: i32,
    swing: f32,
    skin: Px,
    _shade: Px,
    kit: &KitDef,
) {
    let sway = (swing * 2.0).round() as i32;
    c.rect(x, y + sway, 2, 3, kit.body); // sleeve
    c.rect(x, y + 3 + sway, 2, 4, skin); // forearm
    c.px(x, y + 7 + sway, skin);
}

fn draw_head(
    c: &mut Canvas,
    cx: i32,
    head_y: i32,
    look: &Look,
    skin: Px,
    shade: Px,
    hair: &HairDef,
) {
    // face block: 11 wide x 9 tall, cx centered
    c.rect(cx - 5, head_y + 2, 10, 8, skin);
    c.rect(cx + 3, head_y + 2, 2, 8, shade); // right side shade (facing right)
    // ears
    c.px(cx - 6, head_y + 5, skin);
    c.px(cx - 6, head_y + 6, shade);

    // hair by style
    match hair.style {
        HairStyle::Short => {
            c.rect(cx - 5, head_y, 10, 2, hair.color);
            c.rect(cx - 6, head_y + 1, 2, 3, hair.color);
            c.rect(cx + 4, head_y + 1, 2, 2, hair.shade);
            c.rect(cx - 4, head_y + 2, 5, 1, hair.color); // fringe
        }
        HairStyle::Curly => {
            // bumpy curls
            for (i, dx) in [-6, -4, -2, 0, 2, 4].iter().enumerate() {
                let h = if i % 2 == 0 { 3 } else { 2 };
                c.rect(cx + dx, head_y + (2 - h), 2, h + 1, hair.color);
            }
            c.rect(cx - 6, head_y + 2, 2, 2, hair.color);
            c.rect(cx - 4, head_y + 2, 4, 1, hair.shade);
        }
        HairStyle::Quiff => {
            c.rect(cx - 5, head_y + 1, 10, 2, hair.color);
            c.rect(cx - 6, head_y + 2, 2, 2, hair.color);
            // swept-up front tuft (facing right)
            c.rect(cx + 1, head_y - 2, 3, 3, hair.color);
            c.px(cx + 4, head_y - 1, hair.shade);
            c.px(cx + 3, head_y, hair.shade);
            c.rect(cx - 3, head_y + 3, 3, 1, hair.color);
        }
        HairStyle::Wavy => {
            c.rect(cx - 5, head_y, 10, 3, hair.color);
            c.rect(cx - 7, head_y + 1, 2, 6, hair.color); // side volume
            c.rect(cx + 4, head_y + 1, 2, 3, hair.color);
            c.rect(cx - 4, head_y + 3, 6, 1, hair.shade); // wavy fringe line
        }
    }

    // eyes (facing right): white + pupil
    c.px(cx - 1, head_y + 5, px(255, 255, 255));
    c.px(cx - 1, head_y + 6, OUTLINE);
    c.px(cx + 2, head_y + 5, px(255, 255, 255));
    c.px(cx + 2, head_y + 6, OUTLINE);
    // brows
    c.px(cx - 2, head_y + 4, hair.shade);
    c.px(cx + 1, head_y + 4, hair.shade);
    c.px(cx + 2, head_y + 4, hair.shade);
    // smile
    c.px(cx + 1, head_y + 8, shade);
    c.px(cx, head_y + 8, shade);

    // optional glasses (dark frames)
    if look.glasses {
        c.rect(cx - 2, head_y + 5, 2, 2, OUTLINE);
        c.rect(cx + 1, head_y + 5, 3, 2, OUTLINE);
        c.px(cx, head_y + 5, OUTLINE); // bridge
    }

    // optional beard/stubble along jaw
    if look.beard {
        c.rect(cx - 5, head_y + 8, 10, 2, hair.shade);
        c.rect(cx - 5, head_y + 10, 3, 1, hair.shade);
        c.rect(cx + 3, head_y + 10, 2, 1, hair.shade);
    }

    // head outline (top only, avoids boxing the face)
    c.rect(cx - 5, head_y - 1, 10, 1, OUTLINE);
}

const ONE: [&str; 5] = [" # ", "## ", " # ", " # ", " # "];
const ZERO: [&str; 5] = ["###", "# #", "# #", "# #", "###"];

fn draw_number10(c: &mut Canvas, cx: i32, y: i32, color: Px) {
    draw_glyph(c, ONE, cx - 1, y, color);
    draw_glyph(c, ZERO, cx + 2, y, color);
}

fn draw_glyph(c: &mut Canvas, rows: [&str; 5], x: i32, y: i32, color: Px) {
    for (ry, row) in rows.iter().enumerate() {
        for (rx, ch) in row.chars().enumerate() {
            if ch == '#' {
                c.px(x + rx as i32, y + ry as i32, color);
            }
        }
    }
}

fn draw_ball(c: &mut Canvas, x: i32, y: i32, f: u32) {
    let white = px(255, 255, 255);
    let dark = px(40, 44, 52);
    c.rect(x - 2, y, 5, 5, white);
    let spin = f % 3;
    c.px(x, y + 2, dark);
    c.px(x - 1 + spin as i32, y + 1, dark);
    c.px(x + 1 - spin as i32, y + 3, dark);
    for dx in -1..=1 {
        c.px(x + dx, y, dark);
        c.px(x + dx, y + 4, dark);
    }
    c.px(x - 2, y + 1, dark);
    c.px(x - 2, y + 2, dark);
    c.px(x - 2, y + 3, dark);
    c.px(x + 2, y + 1, dark);
    c.px(x + 2, y + 2, dark);
    c.px(x + 2, y + 3, dark);
}

// ---------------------------------------------------------------- packs

fn write_pack(dir: &std::path::Path, look: &Look, name: &str, speed: u32) {
    std::fs::create_dir_all(dir).unwrap();
    for f in 0..6u32 {
        let img = draw_look(look, f, true);
        img.save(dir.join(format!("walk_{:02}.png", f + 1))).unwrap();
        mirror(&img)
            .save(dir.join(format!("walk_left_{:02}.png", f + 1)))
            .unwrap();
    }
    let manifest = serde_json::json!({
        "name": name,
        "description": format!("Custom character ({})", kit_label(look.kit)),
        "frameRate": 10,
        "width": GRID * SCALE,
        "height": GRID * SCALE,
        "walkingSpeed": speed,
        "frames": 6,
    });
    std::fs::write(
        dir.join("character.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    println!("pack written to {}", dir.display());
}

fn kit_label(kit: Kit) -> &'static str {
    match kit {
        Kit::Argentina => "albiceleste #10",
        Kit::Miami => "pink #10",
        Kit::Teal => "teal #10",
    }
}

// ---------------------------------------------------------------- gallery

const CELL: u32 = 48 * SCALE + 20; // 164
const LABEL_H: u32 = 18;

fn gallery_looks() -> Vec<(String, Look)> {
    let mut looks = Vec::new();
    // 25 combos: skin (rows) x hair (cols), argentina kit
    for skin in 0..5 {
        for hair in 0..5 {
            looks.push((
                format!("{}-{}", skin + 1, hair + 1),
                Look { skin, hair, kit: Kit::Argentina, glasses: false, beard: false },
            ));
        }
    }
    // Extras on skin 2 / hair 1 base:
    looks.push(("miami".into(), Look { skin: 1, hair: 0, kit: Kit::Miami, glasses: false, beard: false }));
    looks.push(("teal".into(), Look { skin: 1, hair: 0, kit: Kit::Teal, glasses: false, beard: false }));
    looks.push(("glasses".into(), Look { skin: 1, hair: 0, kit: Kit::Argentina, glasses: true, beard: false }));
    looks.push(("beard".into(), Look { skin: 1, hair: 0, kit: Kit::Argentina, glasses: false, beard: true }));
    looks.push(("glasses+beard".into(), Look { skin: 1, hair: 0, kit: Kit::Argentina, glasses: true, beard: true }));
    looks
}

fn build_sheet(path: &str, looks: Vec<(String, Look)>) {
    let cols = 4usize;
    let rows = looks.len().div_ceil(cols);
    let w = cols as u32 * CELL;
    let h = rows as u32 * (CELL + LABEL_H) + 60;
    let mut sheet = RgbaImage::from_pixel(w, h, TRANSPARENT);
    for y in 0..h {
        for x in 0..w {
            if (x / 8 + y / 8) % 2 == 0 {
                sheet.put_pixel(x, y, Rgba([58, 58, 68, 255]));
            } else {
                sheet.put_pixel(x, y, Rgba([50, 50, 60, 255]));
            }
        }
    }
    for (i, (label, look)) in looks.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let ox = col as u32 * CELL;
        let oy = 60 + row as u32 * (CELL + LABEL_H);
        let art = draw_look(look, 3, true);
        for (x, y, p) in art.enumerate_pixels() {
            sheet.put_pixel(ox + 12 + x, oy + 4 + y, *p);
        }
        let idx = i + 1;
        let text = format!("{idx:02} {label}");
        draw_label(&mut sheet, ox + 10, oy + CELL + 2, &text);
    }
    std::fs::write(path, &encode_png(&sheet)).unwrap();
    println!("sheet written to {path} ({} candidates)", looks.len());
}

fn build_gallery(path: &str) {
    let looks = gallery_looks();
    let cols = 5usize;
    let rows = looks.len().div_ceil(cols);
    let w = cols as u32 * CELL;
    let h = rows as u32 * (CELL + LABEL_H) + 60;
    let mut sheet = RgbaImage::from_pixel(w, h, TRANSPARENT);

    // header band
    for y in 0..44 {
        for x in 0..w {
            sheet.put_pixel(x, y, Rgba([28, 28, 36, 255]));
        }
    }
    // (no text font — labels below each cell carry the info)

    for (i, (label, look)) in looks.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let ox = col as u32 * CELL;
        let oy = 60 + row as u32 * (CELL + LABEL_H);
        let art = draw_look(look, 3, true); // mid-stride pose
        // paste art
        for (x, y, p) in art.enumerate_pixels() {
            sheet.put_pixel(ox + 12 + x, oy + 4 + y, *p);
        }
        // label chip: "NN skin-hair"
        let idx = i + 1;
        let text = format!("{idx:02} {label}");
        draw_label(&mut sheet, ox + 10, oy + CELL + 2, &text);
    }

    std::fs::write(path, &encode_png(&sheet)).unwrap();
    println!("gallery written to {path} ({} looks)", looks.len());
}

const FONT: [(&str, [u8; 5]); 13] = [
    ("0", [0b111,0b101,0b101,0b101,0b111]),
    ("1", [0b010,0b110,0b010,0b010,0b010]),
    ("2", [0b111,0b001,0b111,0b100,0b111]),
    ("3", [0b111,0b001,0b111,0b001,0b111]),
    ("4", [0b101,0b101,0b111,0b001,0b001]),
    ("5", [0b111,0b100,0b111,0b001,0b111]),
    ("6", [0b111,0b100,0b111,0b101,0b111]),
    ("7", [0b111,0b001,0b001,0b001,0b001]),
    ("8", [0b111,0b101,0b111,0b101,0b111]),
    ("9", [0b111,0b101,0b111,0b001,0b111]),
    ("-", [0b000,0b000,0b111,0b000,0b000]),
    ("+", [0b000,0b010,0b111,0b010,0b000]),
    (" ", [0b000,0b000,0b000,0b000,0b000]),
];

fn draw_label(sheet: &mut RgbaImage, x: u32, y: u32, text: &str) {
    let white = px(240, 240, 245);
    let mut cx = x;
    for ch in text.chars() {
        let s = ch.to_string();
        let (_, glyph) = FONT.iter().find(|(c, _)| *c == s).unwrap_or(FONT.last().unwrap());
        for row in 0..5u32 {
            for col in 0..3u32 {
                if glyph[row as usize] & (1 << (2 - col)) != 0 {
                    sheet.put_pixel(cx + col, (y as u32) + row, white);
                }
            }
        }
        cx += 4;
    }
}

fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

// ---------------------------------------------------------------- main

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(sheet) = value_of(&args, "--sheet") {
        // --sheet out.png --looks "4:0:argentina;5:0:argentina;4:2:argentina;3:0:argentina"
        let looks_spec = value_of(&args, "--looks").unwrap_or_else(|| "4:0:argentina;5:0:argentina;4:2:argentina;3:0:argentina".into());
        let looks: Vec<(String, Look)> = looks_spec
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|spec| {
                let parts: Vec<&str> = spec.split(':').collect();
                let skin = parts.first().and_then(|s| s.parse::<usize>().ok()).unwrap_or(4) - 1;
                let hair = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(1) - 1;
                let kit = match parts.get(2).copied().unwrap_or("argentina") {
                    "miami" => Kit::Miami,
                    "teal" => Kit::Teal,
                    _ => Kit::Argentina,
                };
                let glasses = parts.get(3).copied() == Some("g");
                let beard = parts.get(3).copied() == Some("b") || parts.get(4).copied() == Some("b");
                let label = parts.join(":");
                (
                    label,
                    Look { skin: skin.min(4), hair: hair.min(4), kit, glasses, beard },
                )
            })
            .collect();
        build_sheet(&sheet, looks);
        return;
    }

    if args.iter().any(|a| a == "--gallery") {
        let path = value_of(&args, "--gallery").unwrap_or("/tmp/wr-gallery.png".into());
        build_gallery(&path);
        return;
    }

    if args.iter().any(|a| a == "--pack") {
        let dir = std::path::PathBuf::from(value_of(&args, "--pack").unwrap_or("custom".into()));
        let skin = value_of(&args, "--skin").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1) - 1;
        let hair = value_of(&args, "--hair").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1) - 1;
        let kit_name = value_of(&args, "--kit").unwrap_or("argentina".into());
        let kit = match kit_name.as_str() {
            "miami" => Kit::Miami,
            "teal" => Kit::Teal,
            _ => Kit::Argentina,
        };
        let name = value_of(&args, "--name").unwrap_or_else(|| "boyfriend".into());
        let speed = value_of(&args, "--speed").and_then(|v| v.parse::<u32>().ok()).unwrap_or(110);
        let look = Look {
            skin: skin.min(4),
            hair: hair.min(4),
            kit,
            glasses: args.iter().any(|a| a == "--glasses"),
            beard: args.iter().any(|a| a == "--beard"),
        };
        write_pack(&dir, &look, &name, speed);
        return;
    }

    // default: regenerate the bundled footballer (teal, skin 3, hair 1)
    let look = Look { skin: 2, hair: 0, kit: Kit::Teal, glasses: false, beard: false };
    write_pack(
        std::path::Path::new("characters/default-footballer"),
        &look,
        "footballer",
        110,
    );
}

fn value_of(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}
