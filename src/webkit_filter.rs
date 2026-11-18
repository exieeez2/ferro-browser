
use std::ffi::CString;
use glib::translate::ToGlibPtr;
use webkit2gtk_sys::*;

pub struct WebKitFilter {
    pub raw: *mut WebKitUserContentFilter,
}

impl Drop for WebKitFilter {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { webkit_user_content_filter_unref(self.raw); }
        }
    }
}

impl Clone for WebKitFilter {
    fn clone(&self) -> Self {
        unsafe {
            webkit_user_content_filter_ref(self.raw);
            Self { raw: self.raw }
        }
    }
}

unsafe impl Send for WebKitFilter {}
unsafe impl Sync for WebKitFilter {}

unsafe extern "C" fn filter_save_cb(
    source_object: *mut glib::gobject_ffi::GObject,
    res: *mut gtk::gio::ffi::GAsyncResult,
    user_data: glib::ffi::gpointer,
) {
    let mut err = std::ptr::null_mut();
    let filter = webkit_user_content_filter_store_save_finish(source_object as *mut _, res, &mut err);
    let cb = Box::from_raw(user_data as *mut Box<dyn FnOnce(WebKitFilter)>);
    if !filter.is_null() {
        cb(WebKitFilter { raw: filter });
    } else if !err.is_null() {
        glib::ffi::g_error_free(err);
    }
}

pub fn save_filter<F: FnOnce(WebKitFilter) + 'static>(store_dir: &str, filter_id: &str, json_data: &[u8], on_done: F) {
    let c_path = CString::new(store_dir).unwrap();
    let c_id = CString::new(filter_id).unwrap();
    let gbytes = glib::Bytes::from(json_data);
    unsafe {
        let store = webkit_user_content_filter_store_new(c_path.as_ptr());
        let boxed: Box<Box<dyn FnOnce(WebKitFilter)>> = Box::new(Box::new(on_done));
        webkit_user_content_filter_store_save(
            store,
            c_id.as_ptr(),
            gbytes.to_glib_none().0,
            std::ptr::null_mut(),
            Some(filter_save_cb),
            Box::into_raw(boxed) as *mut _,
        );
        glib::gobject_ffi::g_object_unref(store as *mut _);
    }
}

unsafe extern "C" fn filter_load_cb(
    source_object: *mut glib::gobject_ffi::GObject,
    res: *mut gtk::gio::ffi::GAsyncResult,
    user_data: glib::ffi::gpointer,
) {
    let mut err = std::ptr::null_mut();
    let filter = webkit_user_content_filter_store_load_finish(source_object as *mut _, res, &mut err);
    let cb = Box::from_raw(user_data as *mut Box<dyn FnOnce(Option<WebKitFilter>)>);
    if !filter.is_null() {
        cb(Some(WebKitFilter { raw: filter }));
    } else {
        if !err.is_null() {
            glib::ffi::g_error_free(err);
        }
        cb(None);
    }
}

pub fn load_filter<F: FnOnce(Option<WebKitFilter>) + 'static>(store_dir: &str, filter_id: &str, on_done: F) {
    let c_path = CString::new(store_dir).unwrap();
    let c_id = CString::new(filter_id).unwrap();
    unsafe {
        let store = webkit_user_content_filter_store_new(c_path.as_ptr());
        let boxed: Box<Box<dyn FnOnce(Option<WebKitFilter>)>> = Box::new(Box::new(on_done));
        webkit_user_content_filter_store_load(
            store,
            c_id.as_ptr(),
            std::ptr::null_mut(),
            Some(filter_load_cb),
            Box::into_raw(boxed) as *mut _,
        );
        glib::gobject_ffi::g_object_unref(store as *mut _);
    }
}

pub fn attach_to_ucm(ucm: &webkit2gtk::UserContentManager, filter: &WebKitFilter) {
    unsafe {
        webkit_user_content_manager_add_filter(ucm.to_glib_none().0, filter.raw);
    }
}

pub fn detach_from_ucm(ucm: &webkit2gtk::UserContentManager, filter: &WebKitFilter) {
    unsafe {
        webkit_user_content_manager_remove_filter(ucm.to_glib_none().0, filter.raw);
    }
}

pub fn add_user_style_sheet(ucm: &webkit2gtk::UserContentManager, css: &str) {
    use webkit2gtk::UserContentManagerExt;
    let stylesheet = webkit2gtk::UserStyleSheet::new(
        css,
        webkit2gtk::UserContentInjectedFrames::AllFrames,
        webkit2gtk::UserStyleLevel::User,
        &[],
        &[
            "https://*.youtube.com/*",
            "https://youtube.com/*",
            "https://youtu.be/*",
        ],
    );
    ucm.add_style_sheet(&stylesheet);
}

