//! macOS menu bar presence (requirement: users can quit/stop without a
//! terminal). While the daemon runs, a ⚽ status item offers:
//!   Walk now · Pause/Resume all reminders · Show/Hide pet · Quit
//!
//! The daemon hosts this on its main thread; the scheduler loop runs on
//! a background thread (see runner.rs).

use std::cell::OnceCell;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSMenu, NSMenuItem, NSStatusBar,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSString};

use crate::queue::PendingShow;
use crate::runner::Shared;
use wremind_core::paths::Home;
use wremind_core::store::save_reminders;

struct MenuIvars {
    shared: Arc<Shared>,
    bin: std::path::PathBuf,
    home: Home,
    pause_item: OnceCell<Retained<NSMenuItem>>,
    status_item: OnceCell<Retained<objc2_app_kit::NSStatusItem>>,
}

define_class!(
    // SAFETY:
    // - The superclass NSObject allows subclassing.
    // - `MenuTarget` does not implement `Drop`.
    #[unsafe(super = objc2_foundation::NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = MenuIvars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(walkNow:))]
        fn walk_now(&self, _sender: Option<&AnyObject>) {
            let message = {
                let reminders = self.ivars().shared.reminders.lock().unwrap();
                reminders
                    .reminders
                    .iter()
                    .find(|r| r.enabled && !r.completed)
                    .map(|r| r.message())
                    .unwrap_or_else(|| "⚽ Hello! 💙".to_string())
            };
            self.ivars()
                .shared
                .queue
                .lock()
                .unwrap()
                .push(PendingShow {
                    message,
                    character: None,
                    label: "menu".into(),
                });
        }

        #[unsafe(method(togglePause:))]
        fn toggle_pause(&self, _sender: Option<&AnyObject>) {
            let iv = self.ivars();
            let mut reminders = iv.shared.reminders.lock().unwrap();
            let any_enabled = reminders.reminders.iter().any(|r| r.enabled);
            for r in reminders.reminders.iter_mut() {
                r.enabled = !any_enabled;
            }
            let new_state = !any_enabled;
            let list = reminders.clone();
            drop(reminders);
            if save_reminders(&iv.home, &list).is_ok() {
                // daemon hot-reloads via mtime
                if let Some(item) = iv.pause_item.get() {
                    item.setTitle(&NSString::from_str(if new_state {
                        "Pause all reminders"
                    } else {
                        "Resume all reminders"
                    }));
                }
            }
        }

        #[unsafe(method(showPet:))]
        fn show_pet(&self, _sender: Option<&AnyObject>) {
            let iv = self.ivars();
            let cfg = wremind_core::Config::load(&iv.home).unwrap_or_default();
            let name = cfg.character;
            let mut cmd = std::process::Command::new(&iv.bin);
            cmd.args(["__pet", "--character", &name]);
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }
            let _ = cmd
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }

        #[unsafe(method(hidePet:))]
        fn hide_pet(&self, _sender: Option<&AnyObject>) {
            let pid_file = self.ivars().home.root().join("pet.pid");
            if let Some(pid) = std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
            {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .output();
                let _ = std::fs::remove_file(&pid_file);
            }
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            let iv = self.ivars();
            self.hide_pet_inner();
            iv.shared.stop.store(true, Ordering::SeqCst);
            let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
            app.stop(None);
        }
    }
);

impl MenuTarget {
    fn hide_pet_inner(&self) {
        let pid_file = self.ivars().home.root().join("pet.pid");
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .output();
            let _ = std::fs::remove_file(&pid_file);
        }
    }
}

fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<objc2::runtime::Sel>,
    target: &MenuTarget,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(""),
        )
    };
    unsafe { item.setTarget(Some(target)) };
    item
}

/// Install the status item and run the AppKit loop. Blocks until Quit.
pub fn run_menu_bar(shared: Arc<Shared>, bin: std::path::PathBuf, home: Home) {
    let mtm = MainThreadMarker::new().expect("daemon main thread");
    let app = NSApplication::sharedApplication(mtm);
    // Accessory: no Dock icon, but a legitimate menu-bar agent.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // Stop watcher: `walking-reminder stop` (IPC thread) only sets a flag;
    // the AppKit run loop must be ended from the main thread.
    {
        let shared_watcher = shared.clone();
        let app_watcher = NSApplication::sharedApplication(mtm);
        let watcher = block2::RcBlock::new(move |_t: std::ptr::NonNull<objc2_foundation::NSTimer>| {
            if shared_watcher.stop.load(Ordering::SeqCst) {
                app_watcher.stop(None);
                // -[NSApplication stop:] only takes effect once another
                // event is processed; post a wake-up so headless/quiet
                // sessions still exit promptly.
                if let Some(event) = objc2_app_kit::NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                    objc2_app_kit::NSEventType::ApplicationDefined,
                    NSPoint::new(0.0, 0.0),
                    objc2_app_kit::NSEventModifierFlags::empty(),
                    0.0,
                    0,
                    None,
                    0,
                    0,
                    0,
                ) {
                    app_watcher.postEvent_atStart(&event, true);
                }
            }
        });
        // SAFETY: the block only touches atomics and calls app.stop on
        // the main thread (timers fire on the run loop that scheduled them).
        unsafe {
            objc2_foundation::NSTimer::scheduledTimerWithTimeInterval_repeats_block(
                0.5,
                true,
                &watcher,
            );
        }
    }


    let any_enabled = shared
        .reminders
        .lock()
        .unwrap()
        .reminders
        .iter()
        .any(|r| r.enabled);

    let status_item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0);
    if let Some(button) = status_item.button(mtm) {
        button.setTitle(&NSString::from_str("⚽"));
        unsafe {
            let _: () = msg_send![
                &*button,
                setToolTip: &*NSString::from_str("Walking Reminder")
            ];
        }
    }

    let alloc = MenuTarget::alloc(mtm).set_ivars(MenuIvars {
        shared,
        bin,
        home,
        pause_item: OnceCell::new(),
        status_item: OnceCell::new(),
    });
    let target: Retained<MenuTarget> = unsafe { msg_send![super(alloc), init] };

    let menu = NSMenu::new(mtm);

    let status_line = menu_item(mtm, "Walking Reminder", None, &target);
    status_line.setEnabled(false);
    menu.addItem(&status_line);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let walk = menu_item(mtm, "Walk now", Some(objc2::sel!(walkNow:)), &target);
    menu.addItem(&walk);

    let pause_title = if any_enabled {
        "Pause all reminders"
    } else {
        "Resume all reminders"
    };
    let pause = menu_item(mtm, pause_title, Some(objc2::sel!(togglePause:)), &target);
    menu.addItem(&pause);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let show = menu_item(mtm, "Show desktop pet", Some(objc2::sel!(showPet:)), &target);
    menu.addItem(&show);
    let hide = menu_item(mtm, "Hide desktop pet", Some(objc2::sel!(hidePet:)), &target);
    menu.addItem(&hide);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let quit = menu_item(mtm, "Quit Walking Reminder", Some(objc2::sel!(quit:)), &target);
    menu.addItem(&quit);

    status_item.setMenu(Some(&menu));

    // Keep handles alive for the process lifetime.
    let _ = target.ivars().pause_item.set(pause);
    let _ = target.ivars().status_item.set(status_item);
    std::mem::forget(target);

    app.run();
}
