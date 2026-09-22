//! macOS overlay: a borderless, transparent, always-on-top, mouse-ignoring
//! NSWindow. The character + speech bubble are drawn in a custom NSView;
//! an NSTimer on the main run loop advances the walk.
//!
//! Window properties (requirement 8):
//! - no window chrome (borderless)
//! - transparent background (clearColor + opaque=false)
//! - above normal windows (floating level)
//! - never steals keyboard focus (activation policy prohibited; the
//!   window is never made key and the app never activates)
//! - disappears automatically when the walk completes

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBezierPath,
    NSBitmapImageRep, NSBitmapImageFileType, NSColor, NSCompositingOperation, NSFont,
    NSImage, NSMutableParagraphStyle, NSStringDrawingOptions, NSScreen, NSSound, NSView,
    NSWindow, NSWindowStyleMask, NSTextAlignment, NSLineBreakMode,
};
use objc2_foundation::{
    MainThreadMarker, NSData, NSDictionary, NSPoint, NSRect, NSSize, NSString, NSTimer,
};

use crate::anim::WalkPlan;
use crate::character::{Character, Direction};
use crate::ShowOptions;

/// Shared animation state, mutated by the timer, read by drawRect.
struct AnimState {
    t: f64,
    frame_idx: usize,
    finished: bool,
}

/// Immutable draw resources for the view.
struct DrawData {
    frames_right: Vec<Retained<NSImage>>,
    frames_left: Vec<Retained<NSImage>>,
    text: Retained<objc2_foundation::NSAttributedString>,
    bubble_rect: NSRect,
    char_size: f64,
    plan: WalkPlan,
    anim: Rc<RefCell<AnimState>>,
}

#[derive(Default)]
struct WalkerIvars {
    data: OnceCell<Rc<DrawData>>,
}

define_class!(
    // SAFETY:
    // - The superclass NSView allows subclassing.
    // - `WalkerView` does not implement `Drop`.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = WalkerIvars]
    struct WalkerView;

    impl WalkerView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let Some(data) = self.ivars().data.get() else {
                return;
            };
            let bounds: NSRect = unsafe { msg_send![self, bounds] };
            unsafe { draw_scene(data, bounds) };
        }

        #[unsafe(method(wantsLayer))]
        fn wants_layer(&self) -> bool {
            true
        }
    }
);

unsafe fn draw_scene(data: &DrawData, bounds: NSRect) {
    // ---- speech bubble ----
    let br = data.bubble_rect;
    let bubble_path =
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(br, 12.0, 12.0);
    NSColor::colorWithWhite_alpha(1.0, 0.94).setFill();
    bubble_path.fill();
    NSColor::colorWithWhite_alpha(0.82, 0.9).setStroke();
    bubble_path.setLineWidth(1.0);
    bubble_path.stroke();

    // Bubble tail pointing down toward the character.
    let tail = NSBezierPath::bezierPath();
    let tail_x = br.origin.x + br.size.width / 2.0;
    let tail_top = br.origin.y + 2.0;
    tail.moveToPoint(NSPoint::new(tail_x - 6.0, tail_top));
    tail.lineToPoint(NSPoint::new(tail_x + 6.0, tail_top));
    tail.lineToPoint(NSPoint::new(tail_x, tail_top - 9.0));
    tail.closePath();
    NSColor::colorWithWhite_alpha(1.0, 0.94).setFill();
    tail.fill();

    // Text (centered, wrapped, emoji-capable system font).
    let inset: f64 = 8.0;
    let text_rect = NSRect::new(
        NSPoint::new(br.origin.x + inset, br.origin.y + 4.0),
        NSSize::new(br.size.width - inset * 2.0, br.size.height - 8.0),
    );
    let _: () = msg_send![&*data.text, drawInRect: text_rect];

    // ---- ground shadow ----
    let char_w = data.char_size;
    let char_bottom: f64 = 12.0;
    let char_x = bounds.origin.x + (bounds.size.width - char_w) / 2.0;
    let shadow_rect = NSRect::new(
        NSPoint::new(char_x + char_w * 0.12, char_bottom - 4.0),
        NSSize::new(char_w * 0.76, 6.0),
    );
    let shadow = NSBezierPath::bezierPathWithOvalInRect(shadow_rect);
    NSColor::colorWithCalibratedRed_green_blue_alpha(0.1, 0.1, 0.15, 0.14).setFill();
    shadow.fill();

    // ---- character sprite ----
    let frames: &Vec<Retained<NSImage>> = match data.plan.direction {
        Direction::Left if !data.frames_left.is_empty() => &data.frames_left,
        Direction::Left => &data.frames_right,
        Direction::Right => &data.frames_right,
    };
    let idx = {
        let st = data.anim.borrow();
        st.frame_idx
    } % frames.len();
    let sprite = &frames[idx];
    let draw_rect = NSRect::new(
        NSPoint::new(char_x, char_bottom),
        NSSize::new(char_w, data.char_size),
    );
    let zero_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
    let _: () = msg_send![
        &*sprite,
        drawInRect: draw_rect,
        fromRect: zero_rect,
        operation: NSCompositingOperation::SourceOver,
        fraction: 1.0f64
    ];
}

/// Mirror an image horizontally using the modern drawing-handler API.
unsafe fn flipped_image(img: Retained<NSImage>) -> Retained<NSImage> {
    let size = img.size();
    let src = img.clone();
    let handler = block2::RcBlock::new(move |_dst: NSRect| -> objc2::runtime::Bool {
        unsafe {
            let zero_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
            // The handler context is already flipped; we want a horizontal
            // mirror, so scale x by -1 around the width.
            let affine: Retained<objc2_foundation::NSAffineTransform> =
                msg_send![class!(NSAffineTransform), new];
            let _: () = msg_send![&affine, translateXBy: size.width, yBy: 0.0f64];
            let _: () = msg_send![&affine, scaleXBy: -1.0f64, yBy: 1.0f64];
            let _: () = msg_send![&affine, concat];
            let _: () = msg_send![
                &*src,
                drawInRect: NSRect::new(NSPoint::new(0.0, 0.0), size),
                fromRect: zero_rect,
                operation: NSCompositingOperation::SourceOver,
                fraction: 1.0f64
            ];
        }
        objc2::runtime::Bool::YES
    });
    NSImage::imageWithSize_flipped_drawingHandler(size, false, &handler)
}

unsafe fn load_image(path: &std::path::Path) -> Result<Retained<NSImage>, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let data = NSData::dataWithBytes_length(bytes.as_ptr() as *const std::ffi::c_void, bytes.len());
    let image: Option<Retained<NSImage>> =
        msg_send![msg_send![class!(NSImage), alloc], initWithData: &*data];
    image.ok_or_else(|| format!("invalid image {}", path.display()))
}

fn make_attributed(message: &str) -> Retained<objc2_foundation::NSAttributedString> {
    unsafe {
        let font = NSFont::boldSystemFontOfSize(15.0);
        let para = NSMutableParagraphStyle::new();
        para.setAlignment(NSTextAlignment::Center);
        para.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        let color =
            NSColor::colorWithCalibratedRed_green_blue_alpha(0.13, 0.13, 0.16, 1.0);

        let v_font: Retained<AnyObject> = Retained::cast_unchecked(font);
        let v_para: Retained<AnyObject> = Retained::cast_unchecked(para);
        let v_color: Retained<AnyObject> = Retained::cast_unchecked(color);
        let attrs = NSDictionary::from_slices(
            &[
                &*NSString::from_str("NSFont"),
                &*NSString::from_str("NSParagraphStyle"),
                &*NSString::from_str("NSForegroundColor"),
            ],
            &[&*v_font, &*v_para, &*v_color],
        );

        let ns_message = NSString::from_str(message);
        objc2_foundation::NSAttributedString::new_with_attributes(&ns_message, &attrs)
    }
}

fn play_soft_sound() {
    unsafe {
        let name = NSString::from_str("Funk");
        let sound: Option<Retained<NSSound>> =
            msg_send![class!(NSSound), soundNamed: &*name];
        if let Some(s) = sound {
            s.setVolume(0.25);
            let _: bool = s.play();
        }
    }
}

pub fn show_walk(
    opts: ShowOptions,
    screenshot: Option<std::path::PathBuf>,
) -> Result<(), String> {
    unsafe { show_walk_inner(opts, screenshot) }
}

unsafe fn show_walk_inner(
    opts: ShowOptions,
    screenshot: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("renderer must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no menu, cannot become active: pure overlay process.
    app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);

    let screen = NSScreen::mainScreen(mtm)
        .or_else(|| NSScreen::screens(mtm).firstObject())
        .ok_or("no screen available")?;
    let frame = screen.frame();
    let screen_w = frame.size.width as f64;

    let ch: &Character = &opts.character;
    let has_left = !ch.left_frame_paths.is_empty();
    let frame_count = match opts.direction {
        Direction::Left if has_left => ch.left_frame_paths.len(),
        _ => ch.frame_paths.len(),
    };
    let char_size = opts
        .size_override
        .map(|s| s as f64)
        .unwrap_or(ch.def.width.max(ch.def.height) as f64)
        .clamp(48.0, 320.0);
    let speed = opts
        .speed_override
        .map(|s| s as f64)
        .unwrap_or(ch.def.walking_speed as f64);

    // Walk band: near the bottom of the usable screen area.
    let visible = screen.visibleFrame();
    let band_y = visible.origin.y as f64 + visible.size.height as f64 * 0.10 + char_size / 2.0;

    // ---- text measurement ----
    let text = make_attributed(&opts.message);
    let max_text_w: f64 = 340.0;
    let measured: NSRect = msg_send![
        &*text,
        boundingRectWithSize: NSSize::new(max_text_w, 10000.0),
        options: NSStringDrawingOptions::UsesLineFragmentOrigin
    ];
    let text_w = (measured.size.width + 2.0).min(max_text_w);
    let text_h = measured.size.height + 2.0;

    let pad_x: f64 = 18.0;
    let pad_y: f64 = 10.0;
    let bubble_w = (text_w + pad_x * 2.0).max(90.0);
    let bubble_h = text_h + pad_y * 2.0;

    let gap: f64 = 16.0;
    let char_margin_bottom: f64 = 12.0;
    let window_w = bubble_w.max(char_size) + 24.0;
    let window_h = bubble_h + gap + char_size + char_margin_bottom + 8.0;

    // Walk plan: margin = window width, so the character starts fully
    // off-screen and exits fully off-screen without wasted distance.
    let plan = WalkPlan::new(
        screen_w,
        band_y,
        window_w,
        opts.direction,
        ch.def.frame_rate as f64,
        frame_count,
        speed,
        opts.duration_override,
    );

    let bubble_rect = NSRect::new(
        NSPoint::new((window_w - bubble_w) / 2.0, window_h - bubble_h - 4.0),
        NSSize::new(bubble_w, bubble_h),
    );

    // ---- sprite frames ----
    let frames_right: Vec<Retained<NSImage>> = ch
        .frame_paths
        .iter()
        .map(|p| load_image(p))
        .collect::<Result<_, _>>()?;
    let frames_left: Vec<Retained<NSImage>> = if has_left {
        ch.left_frame_paths
            .iter()
            .map(|p| load_image(p))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        frames_right
            .iter()
            .map(|f| flipped_image(f.clone()))
            .collect()
    };

    let anim = Rc::new(RefCell::new(AnimState {
        t: 0.0,
        frame_idx: 0,
        finished: false,
    }));

    let draw_data = Rc::new(DrawData {
        frames_right,
        frames_left,
        text,
        bubble_rect,
        char_size,
        plan: plan.clone(),
        anim: anim.clone(),
    });

    // ---- screenshot mode: render one mid-walk frame and exit ----
    if let Some(path) = screenshot {
        {
            let mut st = anim.borrow_mut();
            let mid = plan.duration / 2.0;
            st.t = mid;
            st.frame_idx = plan.frame_at(mid);
        }
        let data2 = draw_data.clone();
        let handler = block2::RcBlock::new(move |dst: NSRect| -> objc2::runtime::Bool {
            unsafe { draw_scene(&data2, dst) };
            objc2::runtime::Bool::YES
        });
        let img = NSImage::imageWithSize_flipped_drawingHandler(
            NSSize::new(window_w, window_h),
            false,
            &handler,
        );
        let tiff: Retained<NSData> = msg_send![&*img, TIFFRepresentation];
        let reps = NSBitmapImageRep::imageRepsWithData(&tiff);
        let Some(rep) = reps.firstObject() else {
            return Err("failed to create bitmap rep".into());
        };
        let rep: Retained<NSBitmapImageRep> = Retained::downcast(rep)
            .map_err(|_| "unexpected rep type".to_string())?;
        let empty = NSDictionary::new();
        let Some(png) =
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty)
        else {
            return Err("failed to encode PNG".into());
        };
        std::fs::write(&path, png.as_bytes_unchecked())
            .map_err(|e| format!("write {}: {e}", path.display()))?;
        return Ok(());
    }

    // ---- window ----
    let content_rect = NSRect::new(
        NSPoint::new(-window_w, 0.0), // start offscreen
        NSSize::new(window_w, window_h),
    );
    let window: Retained<NSWindow> = msg_send![
        msg_send![class!(NSWindow), alloc],
        initWithContentRect: content_rect,
        styleMask: NSWindowStyleMask::empty(),
        backing: NSBackingStoreType::Buffered,
        defer: false,
    ];
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setLevel(3); // NSFloatingWindowLevel: above normal windows
    window.setIgnoresMouseEvents(true);
    window.setHasShadow(false);
    window.setReleasedWhenClosed(false);

    let alloc = WalkerView::alloc(mtm).set_ivars(WalkerIvars {
        data: OnceCell::from(draw_data),
    });
    let view: Retained<WalkerView> = msg_send![super(alloc), initWithFrame: content_rect];
    window.setContentView(Some(&view));

    if opts.sound {
        play_soft_sound();
    }

    // ---- animation timer on the main run loop ----
    let start = std::time::Instant::now();
    let anim_timer = anim.clone();
    let window_t = window.clone();
    let app_t = app.clone();
    let view_t = view.clone();
    let plan_t = plan.clone();
    let tick = block2::RcBlock::new(
        move |_timer: std::ptr::NonNull<NSTimer>| {
            let t = start.elapsed().as_secs_f64();
            {
                let mut st = anim_timer.borrow_mut();
                if st.finished {
                    return;
                }
                st.t = t;
                st.frame_idx = plan_t.frame_at(t);
            }
            let x = plan_t.x_at(t);
            let origin = NSPoint::new(
                x - window_w / 2.0,
                band_y - char_size / 2.0 - char_margin_bottom,
            );
            window_t.setFrameOrigin(origin);
            let _: () = msg_send![&*view_t, setNeedsDisplay: true];
            if plan_t.done(t) {
                let mut st = anim_timer.borrow_mut();
                st.finished = true;
                window_t.orderOut(None);
                app_t.stop(None);
            }
        },
    );
    let _timer =
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0 / 60.0, true, &tick);

    window.orderFrontRegardless();
    app.run();
    std::process::exit(0)
}

// ===========================================================================
// Desktop pet mode: idles at a remembered spot, draggable anywhere.
// ===========================================================================

use objc2_app_kit::NSEvent;
use std::path::PathBuf;

/// Persisted pet placement (fractions of the screen, resolution-safe).
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PetPlacement {
    x: f64,
    y: f64,
}

impl PetPlacement {
    fn path() -> PathBuf {
        let home = wremind_core::paths::Home::resolve();
        home.root().join("pet.json")
    }
    fn load() -> PetPlacement {
        std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(PetPlacement { x: 0.80, y: 0.22 })
    }
    fn save(&self) {
        let home = wremind_core::paths::Home::resolve();
        let _ = home.ensure();
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }
}

struct PetIvars {
    frames: Vec<Retained<NSImage>>,
    char_size: f64,
    frame_idx: std::rc::Rc<std::cell::RefCell<usize>>,
    /// grab offset (screen pt - window origin) while dragging
    grab: std::rc::Rc<std::cell::RefCell<Option<(f64, f64)>>>,
}

define_class!(
    // SAFETY:
    // - The superclass NSView allows subclassing.
    // - `PetView` does not implement `Drop`.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PetIvars]
    struct PetView;

    impl PetView {
        #[unsafe(method(wantsLayer))]
        fn wants_layer(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: &NSEvent) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let iv = self.ivars();
            let bounds: NSRect = unsafe { msg_send![self, bounds] };
            let char_w = iv.char_size;
            let char_x = bounds.origin.x + (bounds.size.width - char_w) / 2.0;

            // ground shadow
            let shadow_rect = NSRect::new(
                NSPoint::new(char_x + char_w * 0.12, 6.0),
                NSSize::new(char_w * 0.76, 6.0),
            );
            let shadow = NSBezierPath::bezierPathWithOvalInRect(shadow_rect);
            NSColor::colorWithCalibratedRed_green_blue_alpha(0.1, 0.1, 0.15, 0.16).setFill();
            shadow.fill();

            // sprite
            let idx = *iv.frame_idx.borrow() % iv.frames.len();
            let sprite = &iv.frames[idx];
            let draw_rect = NSRect::new(
                NSPoint::new(char_x, 10.0),
                NSSize::new(char_w, iv.char_size),
            );
            let zero_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
            unsafe {
                let _: () = msg_send![
                    &*sprite,
                    drawInRect: draw_rect,
                    fromRect: zero_rect,
                    operation: NSCompositingOperation::SourceOver,
                    fraction: 1.0f64
                ];
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let Some(window) = self.window() else { return };
            let pt = event.locationInWindow();
            let sp = window.convertPointToScreen(pt);
            let frame = window.frame();
            *self.ivars().grab.borrow_mut() = Some((sp.x - frame.origin.x, sp.y - frame.origin.y));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let Some(window) = self.window() else { return };
            let pt = event.locationInWindow();
            let sp = window.convertPointToScreen(pt);
            let g = self.ivars().grab.borrow().clone();
            if let Some((dx, dy)) = g {
                window.setFrameOrigin(NSPoint::new(sp.x - dx, sp.y - dy));
                self.setNeedsDisplay(true);
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            *self.ivars().grab.borrow_mut() = None;
            let Some(window) = self.window() else { return };
            let frame = window.frame();
            let mtm = MainThreadMarker::new().unwrap();
            let scr = NSScreen::mainScreen(mtm).unwrap();
            let screen: NSRect = scr.frame();
            let mut p = PetPlacement {
                x: (frame.origin.x + frame.size.width / 2.0) / screen.size.width,
                y: (frame.origin.y + frame.size.height / 2.0) / screen.size.height,
            };
            p.x = p.x.clamp(0.02, 0.98);
            p.y = p.y.clamp(0.02, 0.98);
            p.save();
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            // dismiss the pet: clean exit
            if let Some(window) = self.window() {
                window.orderOut(None);
            }
            let _ = std::fs::remove_file(PetPlacement::path().with_file_name("pet.pid"));
            std::process::exit(0);
        }
    }
);

pub fn run_pet(character_name: Option<&str>) -> Result<(), String> {
    unsafe { run_pet_inner(character_name) }
}

unsafe fn run_pet_inner(character_name: Option<&str>) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("pet must run on the main thread")?;
    let home = wremind_core::paths::Home::resolve();
    home.ensure().map_err(|e| e.to_string())?;
    Character::ensure_default(&home).map_err(|e| e.to_string())?;
    let cfg = wremind_core::Config::load(&home).unwrap_or_default();
    let name = character_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| cfg.character.clone());
    let character = Character::load(&home, &name).map_err(|e| e.to_string())?;

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);

    let char_size = cfg
        .size
        .map(|s| s as f64)
        .unwrap_or(character.def.width.max(character.def.height) as f64)
        .clamp(64.0, 320.0);

    let screen = NSScreen::mainScreen(mtm)
        .or_else(|| NSScreen::screens(mtm).firstObject())
        .ok_or("no screen available")?;
    let frame = screen.frame();
    let visible = screen.visibleFrame();

    // ---- initial placement from pet.json (or default upper-right) ----
    let p = PetPlacement::load();
    let cx = p.x * frame.size.width as f64 + frame.origin.x as f64;
    let cy = p.y * frame.size.height as f64 + frame.origin.y as f64;

    let frames: Vec<Retained<NSImage>> = character
        .frame_paths
        .iter()
        .map(|path| load_image(path))
        .collect::<Result<_, _>>()?;

    let frame_idx = std::rc::Rc::new(std::cell::RefCell::new(0usize));

    let window_w = char_size + 24.0;
    let window_h = char_size + 28.0;
    let mut origin = NSPoint::new(cx - window_w / 2.0, cy - window_h / 2.0);
    // clamp into visible area
    origin.x = origin
        .x
        .clamp(visible.origin.x as f64 - 40.0, visible.origin.x as f64 + visible.size.width as f64 - window_w + 40.0);
    origin.y = origin
        .y
        .clamp(visible.origin.y as f64 - 20.0, visible.origin.y as f64 + visible.size.height as f64 - window_h + 20.0);

    let content_rect = NSRect::new(origin, NSSize::new(window_w, window_h));
    let window: Retained<NSWindow> = msg_send![
        msg_send![class!(NSWindow), alloc],
        initWithContentRect: content_rect,
        styleMask: NSWindowStyleMask::empty(),
        backing: NSBackingStoreType::Buffered,
        defer: false,
    ];
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setLevel(3); // floating
    window.setIgnoresMouseEvents(false); // the pet is grabbable
    window.setHasShadow(false);
    window.setReleasedWhenClosed(false);

    let alloc = PetView::alloc(mtm).set_ivars(PetIvars {
        frames,
        char_size,
        frame_idx: frame_idx.clone(),
        grab: std::rc::Rc::new(std::cell::RefCell::new(None)),
    });
    let view: Retained<PetView> = msg_send![super(alloc), initWithFrame: content_rect];
    window.setContentView(Some(&view));

    // persist pet pid for `dribble pet --stop`
    let _ = std::fs::write(home.root().join("pet.pid"), std::process::id().to_string());

    // idle animation: slow in-place cycle
    let tick = block2::RcBlock::new(move |_t: std::ptr::NonNull<NSTimer>| {
        let mut idx = frame_idx.borrow_mut();
        *idx = (*idx + 1) % 1_000_000;
        drop(idx);
        view.setNeedsDisplay(true);
    });
    let _timer =
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0 / 3.0, true, &tick);

    window.orderFrontRegardless();
    app.run();
    std::process::exit(0)
}
