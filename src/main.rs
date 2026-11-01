use gtk::gdk;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CenterBox, CheckButton,
    CssProvider, DropDown, Entry, Label, Notebook, Orientation, Scale, Stack,
};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use webkit6::WebView;
use webkit6::prelude::*;

const APP_ID: &str = "com.rustybrowzr.Browser";
const DEFAULT_HOME: &str = "https://www.google.com/";

#[derive(Clone, Serialize, Deserialize)]
struct PageRecord {
    title: String,
    url: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct DownloadRecord {
    name: String,
    path: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Preferences {
    home_page: String,
    search_engine: String,
    zoom: f64,
    #[serde(default = "default_gui_scale")]
    gui_scale: f64,
    #[serde(default = "default_auto_gui_scale")]
    auto_gui_scale: bool,
    #[serde(default = "default_color_theme")]
    color_theme: String,
    #[serde(default)]
    inspector_enabled: bool,
    stretch_mode: bool,
    stretch_amount: f64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            home_page: DEFAULT_HOME.to_owned(),
            search_engine: "DuckDuckGo".to_owned(),
            zoom: 1.0,
            gui_scale: default_gui_scale(),
            auto_gui_scale: default_auto_gui_scale(),
            color_theme: default_color_theme(),
            inspector_enabled: false,
            stretch_mode: false,
            stretch_amount: 1.25,
        }
    }
}

fn default_gui_scale() -> f64 {
    1.0
}

fn default_color_theme() -> String {
    "Charcoal".to_owned()
}

fn default_auto_gui_scale() -> bool {
    true
}

fn main() {
    configure_hyprland_webkit_renderer();
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_window);
    app.run();
}

fn configure_hyprland_webkit_renderer() {
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let on_hyprland =
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() || desktop.contains("hyprland");

    if on_wayland && on_hyprland && std::env::var_os("WEBKIT_DMABUF_RENDERER_FORCE_SHM").is_none() {
        // Keep WebKit compositing enabled while using Wayland-safe shared-memory buffers.
        // This runs before GTK or WebKit starts worker threads.
        unsafe { std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1") };
    }
}

fn build_window(app: &Application) {
    let preferences = Rc::new(RefCell::new(load_preferences()));
    let initial_preferences = preferences.borrow().clone();
    let theme = Rc::new(RefCell::new(install_theme(
        initial_preferences.gui_scale,
        &initial_preferences.color_theme,
    )));
    let (profile_data, profile_cache) = profile_directories();
    let network_session =
        webkit6::NetworkSession::new(profile_data.to_str(), profile_cache.to_str());
    if let Some(cookies) = network_session.cookie_manager() {
        cookies.set_persistent_storage(
            profile_data
                .join("cookies.sqlite")
                .to_string_lossy()
                .as_ref(),
            webkit6::CookiePersistentStorage::Sqlite,
        );
    }
    let browser = WebView::builder().network_session(&network_session).build();
    browser.set_background_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
    let webkit_settings =
        webkit6::prelude::WebViewExt::settings(&browser).expect("WebView has settings");
    webkit_settings.set_enable_javascript(true);
    webkit_settings.set_auto_load_images(true);
    webkit_settings.set_enable_developer_extras(preferences.borrow().inspector_enabled);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("rustybrowzr")
        .default_width(1280)
        .default_height(820)
        .build();

    let frame = GtkBox::new(Orientation::Vertical, 0);
    frame.add_css_class("app-frame");

    let brand_row = GtkBox::new(Orientation::Horizontal, 4);
    brand_row.add_css_class("brand-row");
    let brand_button = Button::new();
    brand_button.add_css_class("brand-home");
    brand_button.set_cursor_from_name(Some("pointer"));
    brand_button.set_tooltip_text(Some("Go to the main menu"));
    let brand_contents = GtkBox::new(Orientation::Horizontal, 4);
    let mark = Label::new(Some("◉"));
    mark.add_css_class("brand-mark");
    let brand = Label::new(Some("rustybrowzr"));
    brand.add_css_class("brand-name");
    brand_contents.append(&mark);
    brand_contents.append(&brand);
    brand_button.set_child(Some(&brand_contents));
    brand_row.append(&brand_button);

    let stack = Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(180);

    let (home_page, start_button, home_settings_button, home_center) =
        build_home_page(preferences.borrow().gui_scale);
    let (
        browser_page,
        tabs,
        address,
        status,
        back,
        forward,
        open_settings,
        go,
        new_tab,
        bookmark,
        bookmarks_button,
        history,
        downloads_button,
    ) = build_browser_page(&browser);
    let (
        settings_page,
        home_entry,
        search_engine,
        zoom_scale,
        gui_scale,
        auto_gui_scale,
        color_theme,
        inspector_toggle,
        open_inspector,
        stretch_toggle,
        stretch_scale,
        save_settings,
        settings_back,
        settings_message,
    ) = build_settings_page(&preferences.borrow());

    stack.add_named(&home_page, Some("home"));
    stack.add_named(&browser_page, Some("browser"));
    let settings_scroll = gtk::ScrolledWindow::new();
    settings_scroll.set_hexpand(true);
    settings_scroll.set_vexpand(true);
    settings_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    settings_scroll.set_child(Some(&settings_page));
    stack.add_named(&settings_scroll, Some("settings"));
    let saved_session = load_session_state();
    if saved_session.is_some() {
        stack.set_visible_child_name("browser");
    } else {
        stack.set_visible_child_name("home");
    }

    frame.append(&brand_row);
    frame.append(&stack);
    window.set_child(Some(&frame));

    let pages = stack.clone();
    brand_button.connect_clicked(move |_| pages.set_visible_child_name("home"));

    let pages = stack.clone();
    let prefs = preferences.clone();
    let tabs_for_start = tabs.clone();
    start_button.connect_clicked(move |_| {
        let prefs = prefs.borrow();
        if let Some(web) = active_webview(&tabs_for_start) {
            web.set_zoom_level(prefs.zoom);
            web.load_uri("https://www.google.com/");
        }
        pages.set_visible_child_name("browser");
    });

    let pages = stack.clone();
    let settings_target = Rc::new(RefCell::new("home".to_owned()));
    let target = settings_target.clone();
    home_settings_button.connect_clicked(move |_| {
        *target.borrow_mut() = "home".to_owned();
        pages.set_visible_child_name("settings");
    });

    let pages = stack.clone();
    let target = settings_target.clone();
    open_settings.connect_clicked(move |_| {
        *target.borrow_mut() = "browser".to_owned();
        pages.set_visible_child_name("settings");
    });

    let pages = stack.clone();
    let prefs = preferences.clone();
    let tabs_for_go = tabs.clone();
    let address_for_go = address.clone();
    let status_for_go = status.clone();
    let navigate = move || {
        let input = address_for_go.text().trim().to_owned();
        if input.is_empty() {
            return;
        }
        let prefs = prefs.borrow();
        let uri = normalize_address(&input, &prefs.search_engine);
        status_for_go.set_text("Loading page…");
        if let Some(web) = active_webview(&tabs_for_go) {
            web.load_uri(&uri);
        }
        pages.set_visible_child_name("browser");
    };
    let nav = navigate.clone();
    go.connect_clicked(move |_| nav());
    address.connect_activate(move |_| navigate());

    let tabs_for_back = tabs.clone();
    back.connect_clicked(move |_| {
        if let Some(web) = active_webview(&tabs_for_back) {
            if web.can_go_back() {
                web.go_back();
            }
        }
    });
    let tabs_for_forward = tabs.clone();
    forward.connect_clicked(move |_| {
        if let Some(web) = active_webview(&tabs_for_forward) {
            if web.can_go_forward() {
                web.go_forward();
            }
        }
    });

    let tabs_for_new = tabs.clone();
    let network_for_new = network_session.clone();
    let prefs_for_new = preferences.clone();
    let address_for_new = address.clone();
    let status_for_new = status.clone();
    let back_for_new = back.clone();
    let forward_for_new = forward.clone();
    new_tab.connect_clicked(move |_| {
        create_tab(
            &tabs_for_new,
            &network_for_new,
            &prefs_for_new,
            &address_for_new,
            &status_for_new,
            &back_for_new,
            &forward_for_new,
        )
    });

    let pages = stack.clone();
    let target = settings_target.clone();
    settings_back.connect_clicked(move |_| {
        let destination = target.borrow().clone();
        pages.set_visible_child_name(&destination);
    });

    let settings_for_inspector_toggle = webkit_settings.clone();
    inspector_toggle.connect_toggled(move |toggle| {
        settings_for_inspector_toggle.set_enable_developer_extras(toggle.is_active());
    });

    let tabs_for_settings = tabs.clone();
    let prefs = preferences.clone();
    let target = settings_target.clone();
    let pages = stack.clone();
    let home_entry_save = home_entry.clone();
    let search_engine_save = search_engine.clone();
    let zoom_save = zoom_scale.clone();
    let stretch_toggle_save = stretch_toggle.clone();
    let stretch_save = stretch_scale.clone();
    let gui_scale_save = gui_scale.clone();
    let color_theme_save = color_theme.clone();
    let auto_scale_save = auto_gui_scale.clone();
    let inspector_toggle_save = inspector_toggle.clone();
    let webkit_settings_save = webkit_settings.clone();
    let theme_save = theme.clone();
    let home_center_save = home_center.clone();
    let settings_page_save = settings_page.clone();
    let applied_scale = Rc::new(Cell::new(-1));
    let applied_scale_save = applied_scale.clone();
    let window_for_scale = window.clone();
    let message = settings_message.clone();
    save_settings.connect_clicked(move |_| {
        let next = Preferences {
            home_page: normalize_address(home_entry_save.text().trim(), "DuckDuckGo"),
            search_engine: selected_search_engine(&search_engine_save),
            zoom: zoom_save.value(),
            gui_scale: gui_scale_save.value(),
            auto_gui_scale: auto_scale_save.is_active(),
            color_theme: selected_color_theme(&color_theme_save),
            inspector_enabled: inspector_toggle_save.is_active(),
            stretch_mode: stretch_toggle_save.is_active(),
            stretch_amount: stretch_save.value(),
        };
        *prefs.borrow_mut() = next.clone();
        save_preferences(&next);
        webkit_settings_save.set_enable_developer_extras(next.inspector_enabled);
        refresh_interface_scale(
            &window_for_scale,
            &prefs,
            &theme_save,
            &home_center_save,
            &settings_page_save,
            &applied_scale_save,
        );
        if let Some(web) = active_webview(&tabs_for_settings) {
            web.set_zoom_level(next.zoom);
            apply_stretch(&web, &next);
        }
        message.set_text("settings saved");
        let destination = target.borrow().clone();
        pages.set_visible_child_name(&destination);
    });

    let tabs_for_inspector = tabs.clone();
    let inspector_message = settings_message.clone();
    open_inspector.connect_clicked(move |_| {
        if let Some(inspector) = active_webview(&tabs_for_inspector).and_then(|web| web.inspector())
        {
            inspector.show();
        } else {
            inspector_message.set_text("enable web inspector and save settings first.");
        }
    });

    bind_tab_events(&browser, &address, &status, &back, &forward, &preferences);
    tabs.connect_switch_page({
        let address = address.clone();
        let status = status.clone();
        let back = back.clone();
        let forward = forward.clone();
        move |tabs, _, _| update_active_tab_ui(tabs, &address, &status, &back, &forward)
    });
    let tabs_for_bookmark = tabs.clone();
    let status_for_bookmark = status.clone();
    bookmark.connect_clicked(move |_| {
        if let Some(web) = active_webview(&tabs_for_bookmark) {
            if let Some(uri) = web.uri() {
                let mut records = load_page_records("bookmarks.json");
                let record = PageRecord {
                    title: web
                        .title()
                        .unwrap_or_else(|| uri.to_string().into())
                        .to_string(),
                    url: uri.to_string(),
                };
                if !records.iter().any(|saved| saved.url == record.url) {
                    records.push(record);
                    save_page_records("bookmarks.json", &records);
                }
                status_for_bookmark.set_text("Bookmark saved");
            }
        }
    });
    let window_for_history = window.clone();
    let tabs_for_history = tabs.clone();
    history.connect_clicked(move |_| {
        show_page_list(
            &window_for_history,
            &tabs_for_history,
            "History",
            load_page_records("history.json"),
        )
    });
    let window_for_bookmarks = window.clone();
    let tabs_for_bookmarks = tabs.clone();
    bookmarks_button.connect_clicked(move |_| {
        show_page_list(
            &window_for_bookmarks,
            &tabs_for_bookmarks,
            "Bookmarks",
            load_page_records("bookmarks.json"),
        )
    });
    let window_for_downloads = window.clone();
    let download_records = Rc::new(RefCell::new(load_download_records()));
    let download_records_button = download_records.clone();
    downloads_button.connect_clicked(move |_| {
        show_download_list(&window_for_downloads, &download_records_button.borrow())
    });
    connect_downloads(&network_session, &status, &download_records);
    if let Some(session) = &saved_session {
        browser.restore_session_state(session);
    }

    let resize_preferences = preferences.clone();
    let resize_theme = theme.clone();
    let resize_home = home_center.clone();
    let resize_settings = settings_page.clone();
    let resize_state = applied_scale.clone();
    window.connect_notify_local(Some("width"), move |window, _| {
        refresh_interface_scale(
            window,
            &resize_preferences,
            &resize_theme,
            &resize_home,
            &resize_settings,
            &resize_state,
        );
    });
    let resize_preferences = preferences.clone();
    let resize_theme = theme.clone();
    let resize_home = home_center.clone();
    let resize_settings = settings_page.clone();
    let resize_state = applied_scale.clone();
    window.connect_notify_local(Some("height"), move |window, _| {
        refresh_interface_scale(
            window,
            &resize_preferences,
            &resize_theme,
            &resize_home,
            &resize_settings,
            &resize_state,
        );
    });

    let prefs_on_close = preferences.clone();
    let home_on_close = home_entry.clone();
    let search_on_close = search_engine.clone();
    let zoom_on_close = zoom_scale.clone();
    let gui_scale_on_close = gui_scale.clone();
    let auto_scale_on_close = auto_gui_scale.clone();
    let theme_on_close = color_theme.clone();
    let inspector_on_close = inspector_toggle.clone();
    let stretch_on_close = stretch_toggle.clone();
    let stretch_scale_on_close = stretch_scale.clone();
    let tabs_on_close = tabs.clone();
    window.connect_close_request(move |_| {
        let current = Preferences {
            home_page: normalize_address(home_on_close.text().trim(), "DuckDuckGo"),
            search_engine: selected_search_engine(&search_on_close),
            zoom: zoom_on_close.value(),
            gui_scale: gui_scale_on_close.value(),
            auto_gui_scale: auto_scale_on_close.is_active(),
            color_theme: selected_color_theme(&theme_on_close),
            inspector_enabled: inspector_on_close.is_active(),
            stretch_mode: stretch_on_close.is_active(),
            stretch_amount: stretch_scale_on_close.value(),
        };
        *prefs_on_close.borrow_mut() = current.clone();
        save_preferences(&current);
        if let Some(web) = active_webview(&tabs_on_close) {
            save_session_state(&web);
        }
        gtk::glib::Propagation::Proceed
    });

    window.present();
    refresh_interface_scale(
        &window,
        &preferences,
        &theme,
        &home_center,
        &settings_page,
        &applied_scale,
    );
}

fn build_home_page(scale: f64) -> (CenterBox, Button, Button, GtkBox) {
    let page = CenterBox::new();
    page.set_hexpand(true);
    page.set_vexpand(true);
    page.add_css_class("welcome-page");

    let center = GtkBox::new(Orientation::Vertical, 16);
    center.set_halign(Align::Center);
    center.set_valign(Align::Center);
    center.set_size_request((620.0 * scale) as i32, -1);
    center.set_spacing((16.0 * scale) as i32);

    let eyebrow = Label::new(Some("a fresh browser"));
    eyebrow.add_css_class("eyebrow");
    let title = Label::new(Some("wanna browse"));
    title.add_css_class("welcome-title");
    let description = Label::new(Some(
        "start browsin does that and use settings to change stuff",
    ));
    description.set_wrap(true);
    description.set_justify(gtk::Justification::Center);
    description.add_css_class("welcome-description");

    let buttons = GtkBox::new(Orientation::Horizontal, 12);
    buttons.set_halign(Align::Center);
    buttons.add_css_class("welcome-actions");
    let start = Button::with_label("start browsin");
    start.set_cursor_from_name(Some("pointer"));
    start.add_css_class("primary-button");
    let settings = Button::with_label("Settings");
    settings.set_cursor_from_name(Some("pointer"));
    settings.add_css_class("secondary-button");
    buttons.append(&start);
    buttons.append(&settings);

    center.append(&eyebrow);
    center.append(&title);
    center.append(&description);
    center.append(&buttons);
    page.set_center_widget(Some(&center));
    (page, start, settings, center)
}

fn build_browser_page(
    browser: &WebView,
) -> (
    GtkBox,
    Notebook,
    Entry,
    Label,
    Button,
    Button,
    Button,
    Button,
    Button,
    Button,
    Button,
    Button,
    Button,
) {
    let page = GtkBox::new(Orientation::Vertical, 0);
    page.add_css_class("browser-page");
    browser.set_hexpand(true);
    browser.set_vexpand(true);

    let toolbar = GtkBox::new(Orientation::Horizontal, 2);
    toolbar.add_css_class("toolbar");
    let back = tool_button("‹", "go back");
    let forward = tool_button("›", "go forward");
    let reload = tool_button("↻", "reload page");
    back.set_sensitive(false);
    forward.set_sensitive(false);

    let address_shell = GtkBox::new(Orientation::Horizontal, 6);
    address_shell.add_css_class("address-shell");
    let address = Entry::new();
    address.set_hexpand(true);
    address.set_placeholder_text(Some("search or enter website"));
    address.add_css_class("address-entry");
    address_shell.append(&address);
    let go = Button::with_label("Go");
    go.set_size_request(38, 17);
    go.set_cursor_from_name(Some("pointer"));
    go.add_css_class("go-button");
    let settings = tool_button("⚙", "setting");
    let new_tab = tool_button("+", "new tab");
    let bookmark = tool_button("★", "bookmark this page");
    let bookmarks = tool_button("▾", "saved bookmarks");
    let history = tool_button("◷", "browsing history");
    let downloads = tool_button("↓", "downloads");

    toolbar.append(&back);
    toolbar.append(&forward);
    toolbar.append(&reload);
    toolbar.append(&address_shell);
    toolbar.append(&go);
    toolbar.append(&new_tab);
    toolbar.append(&bookmark);
    toolbar.append(&bookmarks);
    toolbar.append(&history);
    toolbar.append(&downloads);
    toolbar.append(&settings);
    let status = Label::new(Some("just do it already"));
    status.set_xalign(0.0);
    status.add_css_class("status-label");
    let status_row = GtkBox::new(Orientation::Horizontal, 0);
    status_row.add_css_class("status-row");
    status_row.append(&status);

    page.append(&toolbar);
    let tabs = Notebook::new();
    tabs.set_hexpand(true);
    tabs.set_vexpand(true);
    tabs.set_scrollable(true);
    tabs.append_page(browser, Some(&tab_header(&tabs, browser, "New Tab")));
    let tabs_for_reload = tabs.clone();
    reload.connect_clicked(move |_| {
        if let Some(web) = active_webview(&tabs_for_reload) {
            web.reload();
        }
    });
    page.append(&tabs);
    page.append(&status_row);
    (
        page, tabs, address, status, back, forward, settings, go, new_tab, bookmark, bookmarks,
        history, downloads,
    )
}

fn active_webview(tabs: &Notebook) -> Option<WebView> {
    tabs.nth_page(tabs.current_page())?
        .downcast::<WebView>()
        .ok()
}

fn create_tab(
    tabs: &Notebook,
    session: &webkit6::NetworkSession,
    prefs: &Rc<RefCell<Preferences>>,
    address: &Entry,
    status: &Label,
    back: &Button,
    forward: &Button,
) {
    let web = WebView::builder().network_session(session).build();
    web.set_background_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
    if let Some(settings) = webkit6::prelude::WebViewExt::settings(&web) {
        settings.set_enable_javascript(true);
        settings.set_auto_load_images(true);
        settings.set_enable_developer_extras(prefs.borrow().inspector_enabled);
    }
    web.set_hexpand(true);
    web.set_vexpand(true);
    tabs.append_page(&web, Some(&tab_header(tabs, &web, "New Tab")));
    tabs.set_current_page(Some(tabs.n_pages() - 1));
    bind_tab_events(&web, address, status, back, forward, prefs);
    web.set_zoom_level(prefs.borrow().zoom);
    web.load_uri("https://www.google.com/");
}

fn tab_header(tabs: &Notebook, web: &WebView, title: &str) -> GtkBox {
    let header = GtkBox::new(Orientation::Horizontal, 6);
    let label = Label::new(Some(title));
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_max_width_chars(22);
    let close = Button::with_label("×");
    close.set_cursor_from_name(Some("pointer"));
    close.add_css_class("tab-close");
    let tabs = tabs.clone();
    let web = web.clone();
    close.connect_clicked(move |_| {
        if tabs.n_pages() > 1 {
            if let Some(index) = tabs.page_num(&web) {
                tabs.remove_page(Some(index));
            }
        } else {
            web.load_uri("https://www.google.com/");
        }
    });
    header.append(&label);
    header.append(&close);
    header
}

fn bind_tab_events(
    web: &WebView,
    address: &Entry,
    status: &Label,
    back: &Button,
    forward: &Button,
    prefs: &Rc<RefCell<Preferences>>,
) {
    let address = address.clone();
    let status = status.clone();
    let status_for_error = status.clone();
    let back = back.clone();
    let forward = forward.clone();
    let prefs = prefs.clone();
    web.connect_load_changed(move |web, event| {
        if event == webkit6::LoadEvent::Committed {
            if let Some(uri) = web.uri() {
                address.set_text(uri.as_str());
            }
            status.set_text(web.title().as_deref().unwrap_or("rustybrowzr"));
            back.set_sensitive(web.can_go_back());
            forward.set_sensitive(web.can_go_forward());
            if let Some(child) = web.parent().and_then(|p| p.downcast::<Notebook>().ok()) {
                if let Some(tab_label) = child
                    .tab_label(web)
                    .and_then(|label| label.downcast::<GtkBox>().ok())
                    .and_then(|box_| box_.first_child())
                    .and_then(|label| label.downcast::<Label>().ok())
                {
                    tab_label.set_text(web.title().as_deref().unwrap_or("Loading…"));
                }
            }
        }
        if event == webkit6::LoadEvent::Finished {
            apply_stretch(web, &prefs.borrow());
            if let Some(uri) = web.uri() {
                let url = uri.to_string();
                if url.starts_with("http://") || url.starts_with("https://") {
                    let mut records = load_page_records("history.json");
                    records.retain(|item| item.url != url);
                    records.insert(
                        0,
                        PageRecord {
                            title: web
                                .title()
                                .unwrap_or_else(|| url.clone().into())
                                .to_string(),
                            url,
                        },
                    );
                    records.truncate(500);
                    save_page_records("history.json", &records);
                }
            }
            if let Some(notebook) = web
                .parent()
                .and_then(|parent| parent.downcast::<Notebook>().ok())
            {
                if let Some(label) = notebook
                    .tab_label(web)
                    .and_then(|header| header.downcast::<GtkBox>().ok())
                    .and_then(|header| header.first_child())
                    .and_then(|child| child.downcast::<Label>().ok())
                {
                    label.set_text(web.title().as_deref().unwrap_or("New Tab"));
                }
            }
        }
    });
    web.connect_load_failed(move |web, _, failing_uri, error| {
        status_for_error.set_text(&format!("this page couldnt be loaded: {error}"));
        let safe_uri = html_escape(failing_uri);
        let safe_error = html_escape(&error.to_string());
        let page = format!(
            "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>Page unavailable</title><style>body{{margin:0;background:#000;color:#e7e7e7;font:15px system-ui,sans-serif}}main{{max-width:560px;margin:18vh auto;padding:28px}}h1{{font-size:24px;margin-bottom:8px}}p{{color:#aaa;line-height:1.55}}code{{display:block;overflow-wrap:anywhere;background:#171717;padding:12px;border-radius:6px;color:#c9c9c9}}button{{margin-top:18px;padding:9px 15px;background:#ddd;color:#111;border:0;border-radius:6px;font:inherit;cursor:pointer}}</style></head><body><main><h1>this website cant be reached</h1><p>check the address or your internet, then try again.</p><code>{safe_uri}</code><p>{safe_error}</p><button onclick='location.reload()'>try again</button></main></body></html>"
        );
        web.load_alternate_html(&page, failing_uri, Some(failing_uri));
        true
    });
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn update_active_tab_ui(
    tabs: &Notebook,
    address: &Entry,
    status: &Label,
    back: &Button,
    forward: &Button,
) {
    if let Some(web) = active_webview(tabs) {
        address.set_text(web.uri().as_deref().unwrap_or(""));
        status.set_text(web.title().as_deref().unwrap_or("Ready"));
        back.set_sensitive(web.can_go_back());
        forward.set_sensitive(web.can_go_forward());
    }
}

fn show_page_list(
    parent: &ApplicationWindow,
    tabs: &Notebook,
    title: &str,
    records: Vec<PageRecord>,
) {
    let dialog = gtk::Window::builder()
        .title(title)
        .transient_for(parent)
        .modal(true)
        .default_width(520)
        .default_height(560)
        .build();
    let list = GtkBox::new(Orientation::Vertical, 6);
    list.set_margin_top(14);
    list.set_margin_bottom(14);
    list.set_margin_start(14);
    list.set_margin_end(14);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    for record in records {
        let button = Button::with_label(&format!("{}\n{}", record.title, record.url));
        button.set_cursor_from_name(Some("pointer"));
        button.set_halign(Align::Fill);
        button.add_css_class("secondary-button");
        let tabs = tabs.clone();
        let uri = record.url.clone();
        let dialog = dialog.clone();
        button.connect_clicked(move |_| {
            if let Some(web) = active_webview(&tabs) {
                web.load_uri(&uri);
            }
            dialog.close();
        });
        list.append(&button);
    }
    dialog.set_child(Some(&scroll));
    dialog.present();
}

fn show_download_list(parent: &ApplicationWindow, records: &[DownloadRecord]) {
    let dialog = gtk::Window::builder()
        .title("Downloads")
        .transient_for(parent)
        .modal(true)
        .default_width(520)
        .default_height(420)
        .build();
    let list = GtkBox::new(Orientation::Vertical, 8);
    list.set_margin_top(14);
    list.set_margin_bottom(14);
    list.set_margin_start(14);
    list.set_margin_end(14);
    if records.is_empty() {
        list.append(&Label::new(Some("No downloads yet")));
    }
    for record in records.iter().rev() {
        list.append(&Label::new(Some(&format!(
            "{}\n{}",
            record.name, record.path
        ))));
    }
    dialog.set_child(Some(&list));
    dialog.present();
}

fn connect_downloads(
    session: &webkit6::NetworkSession,
    status: &Label,
    records: &Rc<RefCell<Vec<DownloadRecord>>>,
) {
    let status = status.clone();
    let records = records.clone();
    session.connect_download_started(move |_, download| {
        let downloads = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads");
        let _ = fs::create_dir_all(&downloads);
        let folder = downloads.clone();
        download.connect_decide_destination(move |download, suggested| {
            let name = PathBuf::from(suggested)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut path = folder.join(&name);
            let mut suffix = 1;
            while path.exists() {
                path = folder.join(format!("{} ({suffix})", name));
                suffix += 1;
            }
            let uri = gtk::gio::File::for_path(&path).uri();
            download.set_destination(uri.as_str());
            true
        });
        let status_done = status.clone();
        let records_done = records.clone();
        download.connect_finished(move |download| {
            if let Some(path) = download.destination() {
                let path = gtk::gio::File::for_uri(path.as_str())
                    .path()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string());
                let record = DownloadRecord {
                    name: PathBuf::from(&path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    path: path.clone(),
                };
                let mut saved = records_done.borrow_mut();
                saved.push(record);
                let _ = fs::write(
                    download_records_path(),
                    serde_json::to_vec_pretty(&*saved).unwrap_or_default(),
                );
                status_done.set_text(&format!("Download saved: {path}"));
            }
        });
        let status_failed = status.clone();
        download.connect_failed(move |_, error| {
            status_failed.set_text(&format!("Download failed: {error}"))
        });
        status.set_text("Download started…");
    });
}

fn page_records_path(name: &str) -> PathBuf {
    profile_directories().0.join(name)
}
fn download_records_path() -> PathBuf {
    page_records_path("downloads.json")
}
fn load_page_records(name: &str) -> Vec<PageRecord> {
    fs::read(page_records_path(name))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}
fn save_page_records(name: &str, records: &[PageRecord]) {
    let _ = fs::write(
        page_records_path(name),
        serde_json::to_vec_pretty(records).unwrap_or_default(),
    );
}
fn load_download_records() -> Vec<DownloadRecord> {
    fs::read(download_records_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn build_settings_page(
    preferences: &Preferences,
) -> (
    GtkBox,
    Entry,
    DropDown,
    Scale,
    Scale,
    CheckButton,
    DropDown,
    CheckButton,
    Button,
    CheckButton,
    Scale,
    Button,
    Button,
    Label,
) {
    let page = GtkBox::new(Orientation::Vertical, 18);
    page.set_halign(Align::Center);
    page.set_valign(Align::Start);
    page.set_size_request((650.0 * preferences.gui_scale) as i32, -1);
    page.set_spacing((18.0 * preferences.gui_scale) as i32);
    page.set_margin_top(30);
    page.set_margin_bottom(30);
    page.set_margin_start(26);
    page.set_margin_end(26);
    page.add_css_class("settings-card");

    let heading = Label::new(Some("settings"));
    heading.set_xalign(0.0);
    heading.add_css_class("settings-title");
    let intro = Label::new(Some("rustybrowzin"));
    intro.set_xalign(0.0);
    intro.add_css_class("settings-description");

    let home_entry = Entry::new();
    home_entry.set_text(&preferences.home_page);
    home_entry.set_placeholder_text(Some("https://example.com"));
    home_entry.add_css_class("settings-entry");

    let search_engine = DropDown::from_strings(&["duckduckgo", "google", "bing"]);
    search_engine.set_selected(match preferences.search_engine.as_str() {
        "Google" => 1,
        "Bing" => 2,
        _ => 0,
    });
    search_engine.add_css_class("settings-dropdown");

    let zoom_scale = Scale::with_range(Orientation::Horizontal, 0.75, 1.5, 0.05);
    zoom_scale.set_value(preferences.zoom);
    zoom_scale.set_draw_value(true);
    zoom_scale.set_digits(2);
    zoom_scale.add_css_class("settings-scale");

    let gui_scale = Scale::with_range(Orientation::Horizontal, 0.75, 1.25, 0.05);
    gui_scale.set_value(preferences.gui_scale);
    gui_scale.set_draw_value(true);
    gui_scale.set_digits(2);
    gui_scale.add_css_class("settings-scale");

    let auto_gui_scale = CheckButton::with_label("auto-fit ui to window size");
    auto_gui_scale.set_active(preferences.auto_gui_scale);
    auto_gui_scale.add_css_class("stretch-toggle");
    gui_scale.set_sensitive(!preferences.auto_gui_scale);
    let gui_scale_for_auto = gui_scale.clone();
    auto_gui_scale.connect_toggled(move |toggle| {
        gui_scale_for_auto.set_sensitive(!toggle.is_active());
    });

    let color_theme = DropDown::from_strings(&["charcoal", "midnight", "forest", "light"]);
    color_theme.set_selected(match preferences.color_theme.as_str() {
        "Midnight" => 1,
        "Forest" => 2,
        "Light" => 3,
        _ => 0,
    });
    color_theme.add_css_class("settings-dropdown");

    let inspector_toggle = CheckButton::with_label("enable web inspector");
    inspector_toggle.set_active(preferences.inspector_enabled);
    inspector_toggle.add_css_class("stretch-toggle");
    let inspector_help = Label::new(Some(
        "inspect page elements, scripts, and network activity.",
    ));
    inspector_help.set_xalign(0.0);
    inspector_help.add_css_class("settings-help");
    let open_inspector = Button::with_label("open web inspector");
    open_inspector.set_cursor_from_name(Some("pointer"));
    open_inspector.add_css_class("secondary-button");

    let stretch_toggle = CheckButton::with_label("enable secret stretch mode");
    stretch_toggle.set_active(preferences.stretch_mode);
    stretch_toggle.add_css_class("stretch-toggle");
    let stretch_help = Label::new(Some(
        "widen the webpage like stretched display mode in a game.",
    ));
    stretch_help.set_xalign(0.0);
    stretch_help.add_css_class("settings-help");
    let stretch_scale = Scale::with_range(Orientation::Horizontal, 1.0, 1.5, 0.05);
    stretch_scale.set_value(preferences.stretch_amount);
    stretch_scale.set_draw_value(true);
    stretch_scale.set_digits(2);
    stretch_scale.set_sensitive(preferences.stretch_mode);
    stretch_scale.add_css_class("settings-scale");
    let scale_for_toggle = stretch_scale.clone();
    stretch_toggle.connect_toggled(move |toggle| {
        scale_for_toggle.set_sensitive(toggle.is_active());
    });

    let home_row = setting_row("home page", &home_entry);
    let search_row = setting_row("search engine", &search_engine);
    let zoom_row = setting_row("page zoom", &zoom_scale);
    let gui_scale_row = setting_row("interface size", &gui_scale);
    let gui_scale_help = Label::new(Some(
        "smaller controls and spacing; webpages keep their own zoom.",
    ));
    gui_scale_help.set_xalign(0.0);
    gui_scale_help.add_css_class("settings-help");
    gui_scale_row.append(&gui_scale_help);
    let auto_scale_help = Label::new(Some("ui follows the window size and stays centered."));
    auto_scale_help.set_xalign(0.0);
    auto_scale_help.add_css_class("settings-help");
    let theme_row = setting_row("color theme", &color_theme);
    let divider = GtkBox::new(Orientation::Horizontal, 0);
    divider.add_css_class("settings-divider");
    let secret_label = Label::new(Some("experimental"));
    secret_label.set_xalign(0.0);
    secret_label.add_css_class("eyebrow");
    let stretch_box = GtkBox::new(Orientation::Vertical, 8);
    stretch_box.append(&stretch_toggle);
    stretch_box.append(&stretch_help);
    stretch_box.append(&stretch_scale);

    let message = Label::new(Some(""));
    message.set_xalign(0.0);
    message.add_css_class("settings-message");
    let actions = GtkBox::new(Orientation::Horizontal, 10);
    actions.set_halign(Align::End);
    let back = Button::with_label("back");
    back.set_cursor_from_name(Some("pointer"));
    back.add_css_class("secondary-button");
    let save = Button::with_label("save settings");
    save.set_cursor_from_name(Some("pointer"));
    save.add_css_class("primary-button");
    actions.append(&back);
    actions.append(&save);

    page.append(&heading);
    page.append(&intro);
    page.append(&home_row);
    page.append(&search_row);
    page.append(&zoom_row);
    page.append(&gui_scale_row);
    page.append(&auto_gui_scale);
    page.append(&auto_scale_help);
    page.append(&theme_row);
    page.append(&inspector_toggle);
    page.append(&inspector_help);
    page.append(&open_inspector);
    page.append(&divider);
    page.append(&secret_label);
    page.append(&stretch_box);
    page.append(&message);
    page.append(&actions);

    (
        page,
        home_entry,
        search_engine,
        zoom_scale,
        gui_scale,
        auto_gui_scale,
        color_theme,
        inspector_toggle,
        open_inspector,
        stretch_toggle,
        stretch_scale,
        save,
        back,
        message,
    )
}

fn setting_row<T: IsA<gtk::Widget>>(label_text: &str, control: &T) -> GtkBox {
    let row = GtkBox::new(Orientation::Vertical, 7);
    let label = Label::new(Some(label_text));
    label.set_xalign(0.0);
    label.add_css_class("setting-label");
    row.append(&label);
    row.append(control);
    row
}

fn normalize_address(input: &str, search_engine: &str) -> String {
    if input.contains("://") {
        input.to_owned()
    } else if input.split_whitespace().count() > 1 || !input.contains('.') {
        let query = urlencoding::encode(input);
        match search_engine {
            "Google" => format!("https://www.google.com/search?q={query}"),
            "Bing" => format!("https://www.bing.com/search?q={query}"),
            _ => format!("https://duckduckgo.com/?q={query}"),
        }
    } else {
        format!("https://{input}")
    }
}

fn selected_search_engine(dropdown: &DropDown) -> String {
    match dropdown.selected() {
        1 => "Google".to_owned(),
        2 => "Bing".to_owned(),
        _ => "DuckDuckGo".to_owned(),
    }
}

fn selected_color_theme(dropdown: &DropDown) -> String {
    match dropdown.selected() {
        1 => "Midnight".to_owned(),
        2 => "Forest".to_owned(),
        3 => "Light".to_owned(),
        _ => "Charcoal".to_owned(),
    }
}

fn apply_stretch(browser: &WebView, preferences: &Preferences) {
    let amount = if preferences.stretch_mode {
        preferences.stretch_amount.clamp(1.0, 1.5)
    } else {
        1.0
    };
    let script = format!(
        "(() => {{
          const id = 'rustybrowzr-stretch-style';
          let style = document.getElementById(id);
          if (!style) {{ style = document.createElement('style'); style.id = id; (document.head || document.documentElement).appendChild(style); }}
          const amount = {amount};
          style.textContent = amount <= 1.001 ? '' : `html {{ overflow-x: hidden !important; }} body {{ transform: scaleX(${{amount}}) !important; transform-origin: top left !important; width: ${{100 / amount}}% !important; }}`;
        }})()"
    );
    browser.evaluate_javascript(&script, None, None, None::<&gtk::gio::Cancellable>, |_| {});
}

fn config_directory() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base
}

fn config_path() -> PathBuf {
    config_directory().join("rustybrowzr").join("settings.json")
}

fn profile_directories() -> (PathBuf, PathBuf) {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    let root = base.join("rustybrowzr");
    let data = root.join("data");
    let cache = root.join("cache");
    let _ = fs::create_dir_all(&data);
    let _ = fs::create_dir_all(&cache);
    (data, cache)
}

fn session_state_path() -> PathBuf {
    profile_directories().0.join("session.bin")
}

fn load_session_state() -> Option<webkit6::WebViewSessionState> {
    let bytes = fs::read(session_state_path()).ok()?;
    Some(webkit6::WebViewSessionState::new(
        &gtk::glib::Bytes::from_owned(bytes),
    ))
}

fn save_session_state(browser: &WebView) {
    let Some(state) = browser.session_state() else {
        return;
    };
    let Some(bytes) = state.serialize() else {
        return;
    };
    let _ = fs::write(session_state_path(), bytes.as_ref());
}

fn load_preferences() -> Preferences {
    fs::read(config_path())
        .or_else(|_| fs::read(config_directory().join("tinysurf").join("settings.json")))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_preferences(preferences: &Preferences) {
    let path = config_path();
    if let Some(directory) = path.parent() {
        if fs::create_dir_all(directory).is_err() {
            return;
        }
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(preferences) {
        let _ = fs::write(path, bytes);
    }
}

fn refresh_interface_scale(
    window: &ApplicationWindow,
    preferences: &Rc<RefCell<Preferences>>,
    theme: &RefCell<CssProvider>,
    home_center: &GtkBox,
    settings_page: &GtkBox,
    last_scale: &Cell<i32>,
) {
    let preferences = preferences.borrow().clone();
    let fit = if preferences.auto_gui_scale {
        let width_fit = window.width().max(1) as f64 / 1280.0;
        let height_fit = window.height().max(1) as f64 / 820.0;
        width_fit.min(height_fit).clamp(0.75, 1.25)
    } else {
        1.0
    };
    let scale = (preferences.gui_scale * fit).clamp(0.75, 1.25);
    let scale_key = (scale * 100.0).round() as i32;
    if last_scale.get() == scale_key {
        return;
    }
    last_scale.set(scale_key);

    replace_theme(theme, scale, &preferences.color_theme);
    home_center.set_size_request((620.0 * scale) as i32, -1);
    home_center.set_spacing((16.0 * scale) as i32);
    settings_page.set_size_request((650.0 * scale) as i32, -1);
    settings_page.set_spacing((18.0 * scale) as i32);
    settings_page.set_margin_top((30.0 * scale) as i32);
    settings_page.set_margin_bottom((30.0 * scale) as i32);
    settings_page.set_margin_start((26.0 * scale) as i32);
    settings_page.set_margin_end((26.0 * scale) as i32);
}

fn tool_button(label: &str, tooltip: &str) -> Button {
    let button = Button::with_label(label);
    button.set_cursor_from_name(Some("pointer"));
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("nav-button");
    button
}

fn install_theme(scale: f64, theme: &str) -> CssProvider {
    let css = CssProvider::new();
    let mut css_text = String::from(
        ".app-frame { background: #0d0e11; color: #f4f4f6; }
         .brand-row { padding: 4px 10px 3px; background: #17181b; border-bottom: 1px solid #292a2e; }
         .brand-home { min-height: 25px; padding: 0 5px; border: none; border-radius: 8px; background: transparent; box-shadow: none; }
         .brand-home:hover { background: rgba(255,255,255,0.08); }
         .brand-mark { color: #f0f1f5; font-size: 12px; text-shadow: 0 0 9px rgba(255,255,255,0.3); }
         .brand-name { color: #f5f5f7; font-size: 12px; font-weight: 700; letter-spacing: 0.2px; }
         .tab-chip { background: rgba(255,255,255,0.075); color: #d6d7dc; border: 1px solid rgba(255,255,255,0.10); border-radius: 8px; padding: 3px 8px; margin-left: 4px; font-size: 10px; box-shadow: inset 0 1px rgba(255,255,255,0.09); }
         notebook > header { background: rgba(255,255,255,0.035); border-bottom: 1px solid rgba(255,255,255,0.10); }
         notebook tab { padding: 4px 8px; color: #c7c8ce; }
         .tab-close { min-width: 22px; min-height: 22px; padding: 0; border-radius: 8px; background: transparent; color: #a4a5ad; }
         .tab-close:hover { background: rgba(255,255,255,0.12); color: #ffffff; }
         .welcome-page { background: #111214; }
         .eyebrow { color: #a9abb6; font-size: 10px; font-weight: 700; letter-spacing: 2px; }
         .welcome-title { color: #f4f4f7; font-size: 42px; font-weight: 700; letter-spacing: -1px; }
         .welcome-description { color: #a0a2ab; font-size: 15px; line-height: 1.5; margin-bottom: 14px; }
         .welcome-actions { margin-top: 8px; }
         .toolbar { padding: 0 5px; background: #191a1d; border-bottom: 1px solid #292a2e; }
         .nav-button { min-width: 34px; min-height: 15px; padding: 0 5px; border-radius: 4px; background: #25262a; color: #dedfe3; border: 1px solid #34353a; font-size: 10px; box-shadow: none; }
         .nav-button:hover { background: #303136; }
         .address-shell { min-height: 14px; padding: 0 5px; border-radius: 4px; background: #111214; border: 1px solid #34353a; box-shadow: none; }
         .secure-dot { color: #a8d9c1; font-size: 9px; }
         .address-entry { background: transparent; color: #f0f0f3; border: none; box-shadow: none; padding: 0; font-size: 12px; }
         .address-entry:focus { box-shadow: none; }
         .go-button, .primary-button { min-height: 42px; padding: 0 20px; border-radius: 7px; background: #d6d7da; color: #17181a; border: 1px solid #d6d7da; font-weight: 600; box-shadow: none; }
         .go-button { min-height: 15px; padding: 0 7px; border-radius: 5px; font-size: 10px; }
         .go-button:hover, .primary-button:hover { background: #ffffff; }
         .secondary-button { min-height: 34px; padding: 0 14px; border-radius: 7px; background: #222327; color: #e0e1e4; border: 1px solid #36373c; box-shadow: none; }
         .secondary-button:hover { background: #2d2e33; }
         .status-row { padding: 8px 22px; background: rgba(15,16,20,0.96); border-top: 1px solid rgba(255,255,255,0.09); }
         .status-label { color: #858790; font-size: 11px; }
         .settings-card { padding: 28px; border-radius: 10px; background: #191a1d; border: 1px solid #303136; box-shadow: none; }
         .settings-title { color: #f4f4f7; font-size: 28px; font-weight: 700; }
         .settings-description, .settings-help { color: #9698a2; font-size: 13px; }
         .setting-label { color: #d6d7dc; font-size: 12px; font-weight: 600; }
         .settings-entry { min-height: 38px; padding: 0 12px; border-radius: 12px; background: rgba(5,6,8,0.32); color: #f1f1f3; border: 1px solid rgba(255,255,255,0.14); box-shadow: inset 0 1px rgba(255,255,255,0.045); }
         .settings-dropdown { min-height: 38px; border-radius: 12px; background: rgba(5,6,8,0.32); color: #f1f1f3; border: 1px solid rgba(255,255,255,0.14); }
         .settings-scale { color: #e3e4e9; }
         .stretch-toggle { color: #ececf0; font-size: 13px; }
         .settings-divider { min-height: 1px; background: rgba(255,255,255,0.12); margin-top: 2px; }
         .settings-message { color: #a8d9c1; font-size: 12px; }
         entry, button, dropdown { outline: none; }",
    );
    let scale = scale.clamp(0.75, 1.25);
    let px = |value: f64| (value * scale).round() as i32;
    css_text.push_str(&format!(
        ".brand-row {{ padding: {}px {}px {}px; }}
         .toolbar {{ padding: {}px {}px; }}
         .nav-button {{ min-width: {}px; min-height: {}px; border-radius: {}px; font-size: {}px; }}
         .address-shell {{ min-height: {}px; padding-left: {}px; padding-right: {}px; border-radius: {}px; }}
         .address-entry {{ padding-top: {}px; padding-bottom: {}px; font-size: {}px; }}
         .primary-button {{ min-height: {}px; padding-left: {}px; padding-right: {}px; border-radius: {}px; }}
         .go-button {{ min-height: {}px; padding-left: {}px; padding-right: {}px; border-radius: {}px; }}
         .secondary-button {{ min-height: {}px; padding-left: {}px; padding-right: {}px; border-radius: {}px; }}
         .status-row {{ padding: {}px {}px; }}
         .welcome-title {{ font-size: {}px; }}
         .welcome-description {{ font-size: {}px; }}
         .settings-card {{ padding: {}px; border-radius: {}px; }}
         .settings-title {{ font-size: {}px; }}
         .settings-description, .settings-help {{ font-size: {}px; }}
         .setting-label {{ font-size: {}px; }}
         .settings-entry, .settings-dropdown {{ min-height: {}px; }}
         .stretch-toggle {{ font-size: {}px; }}
         .settings-message, .status-label {{ font-size: {}px; }}",
        px(4.0), px(10.0), px(3.0),
        px(0.0), px(5.0),
        px(34.0), px(15.0), px(4.0), px(10.0),
        px(14.0), px(5.0), px(5.0), px(7.0),
        px(0.0), px(0.0), px(12.0),
        px(42.0), px(20.0), px(20.0), px(15.0),
        px(15.0), px(7.0), px(7.0), px(5.0),
        px(34.0), px(14.0), px(14.0), px(11.0),
        px(8.0), px(22.0),
        px(42.0), px(15.0),
        px(28.0), px(28.0),
        px(28.0),
        px(13.0),
        px(12.0),
        px(38.0),
        px(13.0),
        px(12.0),
    ));
    css_text.push_str(&theme_overrides(theme));
    css.load_from_data(&css_text);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    css
}

fn replace_theme(theme: &RefCell<CssProvider>, scale: f64, color_theme: &str) {
    if let Some(display) = gdk::Display::default() {
        let old = theme.borrow().clone();
        gtk::style_context_remove_provider_for_display(&display, &old);
    }
    *theme.borrow_mut() = install_theme(scale, color_theme);
}

fn theme_overrides(theme: &str) -> String {
    let (frame, chrome, surface, border, text, muted, field, primary, primary_text, accent) =
        match theme {
            "Midnight" => (
                "#0b1020",
                "#10182a",
                "rgba(54,72,112,0.42)",
                "rgba(171,194,242,0.20)",
                "#edf2ff",
                "#a4afc8",
                "rgba(6,12,27,0.72)",
                "#c8d9ff",
                "#17223d",
                "#a9c8ff",
            ),
            "Forest" => (
                "#0c1512",
                "#111e19",
                "rgba(49,83,65,0.42)",
                "rgba(165,207,178,0.20)",
                "#edf5ef",
                "#a7b9ac",
                "rgba(6,18,12,0.72)",
                "#c4e7cf",
                "#16291d",
                "#a5dfb5",
            ),
            "Light" => (
                "#edf0f5",
                "#f7f8fb",
                "rgba(255,255,255,0.78)",
                "rgba(35,43,57,0.15)",
                "#20232b",
                "#686e79",
                "rgba(255,255,255,0.88)",
                "#252a34",
                "#ffffff",
                "#347550",
            ),
            _ => (
                "#0d0e11",
                "#17181c",
                "rgba(255,255,255,0.075)",
                "rgba(255,255,255,0.14)",
                "#f4f4f6",
                "#9698a2",
                "rgba(5,6,8,0.48)",
                "#dedfe3",
                "#191a1e",
                "#a8d9c1",
            ),
        };

    format!(
        ".app-frame {{ background: {frame}; color: {text}; }}
         .brand-row, .toolbar {{ background: {chrome}; border-color: {border}; }}
         .brand-name, .welcome-title, .settings-title {{ color: {text}; }}
         .brand-mark, .eyebrow, .secure-dot {{ color: {accent}; }}
         .tab-chip, .nav-button, .secondary-button {{ background: {surface}; color: {text}; border-color: {border}; }}
         .subtitle, .welcome-description, .settings-description, .settings-help, .status-label {{ color: {muted}; }}
         .welcome-page {{ background: {frame}; }}
         .address-shell, .settings-entry, .settings-dropdown {{ background: {field}; border-color: {border}; }}
         .address-entry {{ color: {text}; }}
         .go-button, .primary-button {{ background: {primary}; color: {primary_text}; border-color: {border}; }}
         .go-button:hover, .primary-button:hover {{ background: {text}; color: {frame}; }}
         .status-row {{ background: {chrome}; border-color: {border}; }}
         .settings-card {{ background: {chrome}; border-color: {border}; box-shadow: none; }}
         .setting-label, .stretch-toggle {{ color: {text}; }}
         .settings-divider {{ background: {border}; }}
         .settings-message {{ color: {accent}; }}"
    )
}
