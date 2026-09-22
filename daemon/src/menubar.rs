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
    NSApplication, NSApplicationActivationPolicy, NSMenu, NSMenuDelegate, NSMenuItem,
    NSStatusBar,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSObjectProtocol, NSString};

use chrono::Utc;
use crate::queue::PendingShow;
use crate::runner::Shared;
use wremind_core::paths::Home;
use wremind_core::recurrence::{humanize, next_due};
use wremind_core::store::save_reminders;


struct MenuIvars {
    shared: Arc<Shared>,
    bin: std::path::PathBuf,
    home: Home,
    menu: OnceCell<Retained<NSMenu>>,
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

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for MenuTarget {}

    // SAFETY: delegate method signature matches the protocol.
    unsafe impl NSMenuDelegate for MenuTarget {
        #[unsafe(method(menuNeedsUpdate:))]
        #[allow(non_snake_case)]
        fn menuNeedsUpdate(&self, _menu: &NSMenu) {
            if let Some(mtm) = MainThreadMarker::new() {
                self.rebuild_menu(mtm);
            }
        }
    }

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
            let list = reminders.clone();
            drop(reminders);
            if save_reminders(&iv.home, &list).is_ok() {
                // daemon hot-reloads via mtime; menu refreshes on next open
            }
        }

        #[unsafe(method(toggleReminder:))]
        fn toggle_reminder(&self, sender: Option<&AnyObject>) {
            let tag: isize = match sender {
                Some(s) => unsafe { msg_send![s, tag] },
                None => return,
            };
            let iv = self.ivars();
            let mut reminders = iv.shared.reminders.lock().unwrap();
            let Some(r) = reminders.reminders.iter_mut().find(|r| r.id as isize == tag) else {
                return;
            };
            r.enabled = !r.enabled;
            let list = reminders.clone();
            drop(reminders);
            let _ = save_reminders(&iv.home, &list);
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

// SAFETY: `MenuTarget` is an NSObject subclass; method signature matches
// the optional protocol method `menuNeedsUpdate:`.
impl MenuTarget {
    /// Rebuild the whole menu (called on every open via menuNeedsUpdate,
    /// so reminder list / status / pause title stay fresh).
    fn rebuild_menu(&self, mtm: MainThreadMarker) {
        let Some(menu) = self.ivars().menu.get() else { return };

        let (list_snapshot, state_snapshot) = {
            let reminders = self.ivars().shared.reminders.lock().unwrap();
            let state = self.ivars().shared.state.lock().unwrap();
            (reminders.clone(), state.clone())
        };
        let now = Utc::now();

        menu.removeAllItems();

        // Header with next-fire summary
        let next = wremind_core::recurrence::next_upcoming(
            list_snapshot.reminders.iter(),
            &state_snapshot,
            now,
        );
        let header = match next {
            Some((r, t)) => format!(
                "Dribble — next: {} in {}",
                r.title,
                wremind_core::util::humanize_duration_short((t - now).num_seconds())
            ),
            None => "Dribble".to_string(),
        };
        let status_line = menu_item(mtm, &header, None, self);
        status_line.setEnabled(false);
        menu.addItem(&status_line);

        // ---- reminders list ----
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let title = menu_item(mtm, "Reminders", None, self);
        title.setEnabled(false);
        menu.addItem(&title);

        if list_snapshot.reminders.is_empty() {
            let none = menu_item(mtm, "  (none — add from the terminal)", None, self);
            none.setEnabled(false);
            menu.addItem(&none);
        }
        for r in list_snapshot.reminders.iter().take(20) {
            let next_str = if r.completed {
                "done".to_string()
            } else if !r.enabled {
                "paused".to_string()
            } else {
                match next_due(r, &state_snapshot, now) {
                    Some(t) if t <= now => "due now".to_string(),
                    Some(t) => format!(
                        "in {}",
                        wremind_core::util::humanize_duration_short((t - now).num_seconds())
                    ),
                    None => "—".to_string(),
                }
            };
            let dot = if r.completed {
                "✓"
            } else if r.enabled {
                "●"
            } else {
                "○"
            };
            let line = format!(
                "{dot} {} {} — {} · {next_str}",
                r.emoji.clone().unwrap_or_default(),
                r.title,
                humanize(&r.schedule)
            );
            let item = menu_item(mtm, &line, Some(objc2::sel!(toggleReminder:)), self);
            item.setTag(r.id as isize);
            item.setEnabled(!r.completed);
            menu.addItem(&item);
        }

        // ---- controls ----
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let walk = menu_item(mtm, "Walk now", Some(objc2::sel!(walkNow:)), self);
        menu.addItem(&walk);

        let any_enabled = list_snapshot.reminders.iter().any(|r| r.enabled);
        let pause = menu_item(
            mtm,
            if any_enabled {
                "Pause all reminders"
            } else {
                "Resume all reminders"
            },
            Some(objc2::sel!(togglePause:)),
            self,
        );
        menu.addItem(&pause);

        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let show = menu_item(mtm, "Show desktop pet", Some(objc2::sel!(showPet:)), self);
        menu.addItem(&show);
        let hide = menu_item(mtm, "Hide desktop pet", Some(objc2::sel!(hidePet:)), self);
        menu.addItem(&hide);

        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quit = menu_item(mtm, "Quit Dribble", Some(objc2::sel!(quit:)), self);
        menu.addItem(&quit);
    }

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

    // Stop watcher: `dribble stop` (IPC thread) only sets a flag;
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


    let status_item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0);
    if let Some(button) = status_item.button(mtm) {
        button.setTitle(&NSString::from_str("⚽"));
        unsafe {
            let _: () = msg_send![
                &*button,
                setToolTip: &*NSString::from_str("Dribble")
            ];
        }
    }

    let alloc = MenuTarget::alloc(mtm).set_ivars(MenuIvars {
        shared,
        bin,
        home,
        menu: OnceCell::new(),
        status_item: OnceCell::new(),
    });
    let target: Retained<MenuTarget> = unsafe { msg_send![super(alloc), init] };

    let menu = NSMenu::new(mtm);
    menu.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*target)));

    status_item.setMenu(Some(&menu));

    // Keep handles alive for the process lifetime, and build the menu
    // contents once so the first open is instant (refreshed on every
    // subsequent open by menuNeedsUpdate).
    let _ = target.ivars().menu.set(menu);
    let _ = target.ivars().status_item.set(status_item);
    if let Some(m2) = MainThreadMarker::new() {
        target.rebuild_menu(m2);
    }
    std::mem::forget(target);

    app.run();
}
