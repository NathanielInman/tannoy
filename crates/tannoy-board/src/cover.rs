//! `Cover`: a fixed-size square that shows an image cropped to fill it
//! (a 16:9 video thumbnail loses its sides, as album art would), with
//! rounded corners. `gtk::Picture` sizes itself from the image instead.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use std::cell::{Cell, RefCell};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Cover {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub size: Cell<i32>,
        pub radius: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Cover {
        const NAME: &'static str = "TnCover";
        type Type = super::Cover;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Cover {}

    impl WidgetImpl for Cover {
        fn measure(&self, _o: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let s = self.size.get();
            (s, s, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let Some(tex) = self.texture.borrow().clone() else {
                return;
            };
            let (tw, th) = (tex.width() as f32, tex.height() as f32);
            if tw <= 0.0 || th <= 0.0 || w <= 0.0 || h <= 0.0 {
                return;
            }
            // Cover: scale until both sides fill, then center.
            let scale = (w / tw).max(h / th);
            let (dw, dh) = (tw * scale, th * scale);
            let rect = graphene::Rect::new(0.0, 0.0, w, h);
            let r = self.radius.get();
            let corner = graphene::Size::new(r, r);
            snapshot
                .push_rounded_clip(&gsk::RoundedRect::new(rect, corner, corner, corner, corner));
            snapshot.append_texture(
                &tex,
                &graphene::Rect::new((w - dw) / 2.0, (h - dh) / 2.0, dw, dh),
            );
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct Cover(ObjectSubclass<imp::Cover>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Cover {
    pub fn new(size: i32, radius: f32) -> Cover {
        let c: Cover = glib::Object::new();
        c.imp().size.set(size);
        c.imp().radius.set(radius);
        c
    }

    /// Shows the image at `path`; returns false when it cannot be read.
    pub fn set_path(&self, path: Option<&str>) -> bool {
        let tex = path.and_then(|p| gdk::Texture::from_filename(p).ok());
        let ok = tex.is_some();
        *self.imp().texture.borrow_mut() = tex;
        self.queue_draw();
        ok
    }
}
