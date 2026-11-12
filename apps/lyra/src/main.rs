//! Lyra: YouTube Music for NyxOS.

mod accounts;
mod art;
mod cache;
mod control;
mod dev;
mod icons;
mod mpris;
mod player;
mod prefs;
mod runtime;
mod theme;
mod ui;

use gtk::{gdk, gio, glib, prelude::*};
use player::Player;
use runtime::webkit::WebRuntime;
use std::cell::OnceCell;
use std::rc::Rc;

pub const APP_ID: &str = "io.github.design_nexus.nyx.Lyra";

thread_local! {
    static PLAYER: OnceCell<Rc<Player>> = const { OnceCell::new() };
    static PAGE: OnceCell<gtk::Widget> = const { OnceCell::new() };
}

pub fn player() -> Option<Rc<Player>> {
    PLAYER.with(|p| p.get().cloned())
}

/// Lyra's own symbolic icons, written to the cache once and added to the icon search path.
fn install_icons() {
    const APP: (&str, &str) = (
        "io.github.design_nexus.nyx.Lyra-symbolic.svg",
        include_str!("../data/icons/io.github.design_nexus.nyx.Lyra-symbolic.svg"),
    );
    let dir = nyx_core::paths::cache_dir().join("icons");
    let _ = std::fs::create_dir_all(&dir);
    for (name, svg) in std::iter::once(&APP).chain(icons::ICONS) {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = std::fs::write(&path, svg);
        }
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

/// One-time setup in the primary instance: the theme, the web runtime and the player.
fn start(app: &gtk::Application) {
    theme::install();
    install_icons();
    let player = Player::new();
    let web = nyx_core::paths::data_dir().join("web");
    let cache = nyx_core::paths::cache_dir().join("web");
    let p = Rc::downgrade(&player);
    let rt = WebRuntime::new(
        &web,
        &cache,
        Rc::new(move |e| {
            if let Some(p) = p.upgrade() {
                p.on_runtime(e)
            }
        }),
    );
    if std::env::var("NYX_LYRA_MUTE").as_deref() == Ok("1") {
        runtime::Runtime::set_muted(&*rt, true);
    }
    PAGE.with(|w| w.set(runtime::Runtime::widget(&*rt)).ok());
    PLAYER.with(|c| c.set(player.clone()).ok());
    player.attach(rt);
    if dev::enabled() {
        dev::start(&player, app);
    }
    // The nyx command, Glimpse and Nyx Assistant; media keys, the bar, Control Center, the dock.
    control::serve(app);
    mpris::start(app);
}

/// The window. Closing it while music plays hides it (unless that's turned off); the music
/// keeps going and the launcher or dock brings it back. Closing it otherwise quits.
fn present(app: &gtk::Application) {
    if let Some(w) = app.windows().into_iter().find(|w| w.has_css_class("lyra-window") && !w.has_css_class("lyra-mini")) {
        if !dev::windowless() {
            w.present();
        }
        return;
    }
    let page = PAGE.with(|w| w.get().cloned()).expect("started");
    let window = ui::build(app, &page);
    let a = app.clone();
    window.connect_close_request(move |w| {
        let playing = player().is_some_and(|p| p.state().is_playing());
        if playing && prefs::get().keep_playing {
            w.set_visible(false);
        } else {
            prefs::flush();
            a.quit();
        }
        glib::Propagation::Stop
    });
    // Tests build the window hidden and show it once the rules are checked (tools/headless.sh).
    if !dev::windowless() {
        window.present();
    }
}

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App { slug: "nyx-lyra", name: "Lyra" });
    let id = std::env::var("NYX_LYRA_APP_ID").unwrap_or_else(|_| APP_ID.to_string());
    nyx_core::control::describe_if_asked(&control::description(&id));
    // GTK's Vulkan renderer wakes a sleeping discrete GPU on hybrid laptops; GL doesn't.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded; nothing else reads the environment yet.
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }
    glib::set_application_name("Lyra");
    // Started by D-Bus for an action (the nyx command, Glimpse): no window until one's needed.
    let service = std::env::args().any(|a| a == "--gapplication-service");
    let mut flags = gio::ApplicationFlags::empty();
    if service {
        flags |= gio::ApplicationFlags::IS_SERVICE;
    }
    let app = gtk::Application::builder().application_id(&id).flags(flags).build();
    app.connect_startup(move |app| {
        start(app);
        if service {
            app.set_inactivity_timeout(10_000);
        }
    });
    app.connect_activate(present);
    app.connect_shutdown(|_| prefs::flush());
    app.run_with_args(&["nyx-lyra"])
}
