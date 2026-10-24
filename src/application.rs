use crate::config::APP_ID;
use crate::window::CropWindow;
use gdk4::glib;
use gio::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CropApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for CropApplication {
        const NAME: &'static str = "CropApplication";
        type Type = super::CropApplication;
        type ParentType = libadwaita::Application;
    }

    impl ObjectImpl for CropApplication {}
    impl ApplicationImpl for CropApplication {
        fn activate(&self) {
            let app = self.obj();
            if let Some(win) = app.active_window() {
                win.present();
            } else {
                let win = CropWindow::new(&*app);
                win.present();
            }
        }

        fn startup(&self) {
            self.parent_startup();
            let app = self.obj();
            app.setup_gactions();
            app.setup_accels();
        }
    }
    impl GtkApplicationImpl for CropApplication {}
    impl AdwApplicationImpl for CropApplication {}
}

glib::wrapper! {
    pub struct CropApplication(ObjectSubclass<imp::CropApplication>)
        @extends gio::Application, gtk4::Application, libadwaita::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for CropApplication {
    fn default() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build()
    }
}

impl CropApplication {
    pub fn new() -> Self {
        Self::default()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::SimpleAction::new("quit", None);
        let app_clone = self.clone();
        quit_action.connect_activate(move |_, _| {
            app_clone.quit();
        });
        self.add_action(&quit_action);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("win.open", &["<Control>o"]);
        self.set_accels_for_action("win.save", &["<Control>s"]);
        self.set_accels_for_action("win.show-help-overlay", &["<Control>question"]);
        self.set_accels_for_action("app.quit", &["<Control>q"]);
    }
}

