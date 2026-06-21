use gtk::glib;
use gtk::glib::{ControlFlow, Priority, Propagation};
use gtk::prelude::*;
use webkit2gtk::{WebView, WebViewExt};

pub enum WebviewCmd {
    Open(String),
}

#[derive(Clone)]
pub struct WebviewHandle {
    sender: glib::Sender<WebviewCmd>,
}

impl WebviewHandle {
    pub fn open(&self, url: String) {
        let _ = self.sender.send(WebviewCmd::Open(url));
    }
}

/// Spawns a background GTK thread with a webkit2gtk window.
/// Must be called AFTER iced has started so winit has already called XInitThreads.
pub fn spawn() -> Result<WebviewHandle, String> {
    let (sender, receiver) =
        glib::MainContext::channel::<WebviewCmd>(Priority::DEFAULT);

    std::thread::Builder::new()
        .name("gtk-webview".into())
        .spawn(move || {
            if gtk::init().is_err() {
                eprintln!("monotabe: failed to initialize GTK for webview");
                return;
            }

            let window = gtk::Window::new(gtk::WindowType::Toplevel);
            window.set_title("Monotabe — Media Player");
            window.set_default_size(960, 600);

            let webview = WebView::new();
            window.add(&webview);

            // Hide on close instead of destroying so subsequent opens reuse it.
            // Load about:blank to stop any playing audio/video.
            let webview_hide = webview.clone();
            window.connect_delete_event(move |w, _| {
                webview_hide.load_uri("about:blank");
                w.hide();
                Propagation::Stop
            });

            let window_r = window.clone();
            let webview_r = webview.clone();
            receiver.attach(None, move |cmd: WebviewCmd| {
                match cmd {
                    WebviewCmd::Open(url) => {
                        webview_r.load_uri(&url);
                        window_r.show_all();
                        window_r.present();
                    }
                }
                ControlFlow::Continue
            });

            gtk::main();
        })
        .map_err(|e| e.to_string())?;

    Ok(WebviewHandle { sender })
}
