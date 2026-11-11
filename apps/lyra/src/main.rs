//! Lyra: YouTube Music for NyxOS.

mod dev;
mod player;
mod runtime;

use gtk::{gio, glib, prelude::*};
use player::Player;
use runtime::webkit::WebRuntime;
use std::cell::OnceCell;
use std::rc::Rc;

pub const APP_ID: &str = "io.github.design_nexus.nyx.Lyra";

thread_local! {
    static PLAYER: OnceCell<Rc<Player>> = const { OnceCell::new() };
    static PAGE: OnceCell<gtk::Widget> = const { OnceCell::new() };
}

/// One-time setup in the primary instance: the web runtime and the player.
fn start(app: &gtk::Application) {
    let player = Player::new();
    let web = nyx_core::paths::data_dir().join("web");
    let cache = nyx_core::paths::cache_dir().join("web");
    let p = Rc::downgrade(&player);
    let rt = WebRuntime::new(&web, &cache, Rc::new(move |e| {
        if let Some(p) = p.upgrade() {
            p.on_runtime(e)
        }
    }));
    if std::env::var("NYX_LYRA_MUTE").as_deref() == Ok("1") {
        runtime::Runtime::set_muted(&*rt, true);
    }
    PAGE.with(|w| w.set(runtime::Runtime::widget(&*rt)).ok());
    player.attach(rt);
    if dev::enabled() {
        dev::start(&player, app);
    }
    PLAYER.with(|c| c.set(player).ok());
}

/// The window. Closing it while music plays hides it; the music keeps going and the
/// launcher or dock brings it back. Closing it otherwise quits.
fn present(app: &gtk::Application) {
    if let Some(w) = app.windows().first() {
        w.present();
        return;
    }
    let page = PAGE.with(|w| w.get().cloned()).expect("started");
    let window = gtk::ApplicationWindow::builder().application(app).title("Lyra").default_width(1280).default_height(800).child(&page).build();
    let a = app.clone();
    window.connect_close_request(move |w| {
        let playing = PLAYER.with(|p| p.get().is_some_and(|p| p.state().is_playing()));
        if playing {
            w.set_visible(false);
        } else {
            a.quit();
        }
        glib::Propagation::Stop
    });
    window.present();
}

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App { slug: "nyx-lyra", name: "Lyra" });
    let id = std::env::var("NYX_LYRA_APP_ID").unwrap_or_else(|_| APP_ID.to_string());
    // GTK's Vulkan renderer wakes a sleeping discrete GPU on hybrid laptops; GL doesn't.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded; nothing else reads the environment yet.
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }
    glib::set_application_name("Lyra");
    let flags = if dev::enabled() { gio::ApplicationFlags::NON_UNIQUE } else { gio::ApplicationFlags::empty() };
    let app = gtk::Application::builder().application_id(&id).flags(flags).build();
    app.connect_startup(start);
    app.connect_activate(present);
    app.run_with_args(&["nyx-lyra"])
}
