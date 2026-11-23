//! File ▸ Print… of a PDF, outside macOS: the PDF file itself sent to the printer chosen in GTK's print dialog (its
//! text kept as text, at the printer's resolution), not pictures of its pages. GTK's Unix print dialog is in
//! libgtk-3, which the app links already, but not in gtk-rs: its few functions declared here.

#[cfg(target_os = "linux")]
mod gtk_unix_print {
    use gtk::glib::ffi::{GError, gboolean, gpointer};
    use gtk::ffi::{GtkPageSetup, GtkPrintSettings, GtkWidget, GtkWindow};
    use std::os::raw::c_char;

    #[repr(C)]
    pub struct GtkPrinter {
        _private: [u8; 0],
    }
    #[repr(C)]
    pub struct GtkPrintJob {
        _private: [u8; 0],
    }
    pub type Done = unsafe extern "C" fn(job: *mut GtkPrintJob, data: gpointer, error: *const GError);

    #[link(name = "gtk-3")]
    unsafe extern "C" {
        pub fn gtk_print_unix_dialog_new(title: *const c_char, parent: *mut GtkWindow) -> *mut GtkWidget;
        pub fn gtk_print_unix_dialog_get_selected_printer(dialog: *mut GtkWidget) -> *mut GtkPrinter;
        pub fn gtk_print_unix_dialog_get_settings(dialog: *mut GtkWidget) -> *mut GtkPrintSettings;
        pub fn gtk_print_unix_dialog_get_page_setup(dialog: *mut GtkWidget) -> *mut GtkPageSetup;
        pub fn gtk_printer_accepts_pdf(printer: *mut GtkPrinter) -> gboolean;
        pub fn gtk_print_job_new(
            title: *const c_char,
            printer: *mut GtkPrinter,
            settings: *mut GtkPrintSettings,
            page_setup: *mut GtkPageSetup,
        ) -> *mut GtkPrintJob;
        pub fn gtk_print_job_set_source_file(job: *mut GtkPrintJob, filename: *const c_char, error: *mut *mut GError) -> gboolean;
        pub fn gtk_print_job_send(job: *mut GtkPrintJob, done: Option<Done>, data: gpointer, destroy: Option<unsafe extern "C" fn(gpointer)>);
    }
}

/// What came of the dialog: printed (sent), cancelled, or the printer chosen takes no PDF (the page's own way then).
#[derive(serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Printed {
    Sent,
    Cancelled,
    Unsupported,
}

/// The PDF at `path` printed by GTK's print dialog, on the main thread (the dialog's); its answer once closed.
#[tauri::command]
pub async fn print_pdf(window: tauri::Window, path: String) -> Result<Printed, String> {
    #[cfg(target_os = "linux")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let shown = window.clone();
        window
            .run_on_main_thread(move || {
                let _ = tx.send(dialog(&shown, &path));
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, path);
        Ok(Printed::Unsupported)
    }
}

#[cfg(target_os = "linux")]
fn dialog(window: &tauri::Window, path: &str) -> Result<Printed, String> {
    use gtk::glib::ffi::{GError, gpointer};
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::*;
    use gtk_unix_print::*;
    use std::ffi::CString;

    let parent = window.gtk_window().map_err(|e| e.to_string())?;
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (title, file) = (CString::new(format!("Print {name}")).unwrap_or_default(), CString::new(path).map_err(|e| e.to_string())?);
    // SAFETY: GTK's own calls on the main thread, each object used while the dialog holds it; the job given to GTK,
    // which keeps it while it sends and frees it after (its callback)
    unsafe {
        let parent: *mut gtk::ffi::GtkWindow = parent.upcast_ref::<gtk::Window>().to_glib_none().0;
        let dialog = gtk_print_unix_dialog_new(title.as_ptr(), parent);
        let answer = gtk::ffi::gtk_dialog_run(dialog as *mut _);
        let printed = if answer != gtk::ffi::GTK_RESPONSE_OK {
            Ok(Printed::Cancelled)
        } else {
            let printer = gtk_print_unix_dialog_get_selected_printer(dialog);
            if printer.is_null() || gtk_printer_accepts_pdf(printer) == 0 {
                Ok(Printed::Unsupported)
            } else {
                let job = gtk_print_job_new(
                    title.as_ptr(),
                    printer,
                    gtk_print_unix_dialog_get_settings(dialog),
                    gtk_print_unix_dialog_get_page_setup(dialog),
                );
                let mut error = std::ptr::null_mut();
                if gtk_print_job_set_source_file(job, file.as_ptr(), &mut error) == 0 {
                    let said = if error.is_null() { "the file could not be read".into() } else { std::ffi::CStr::from_ptr((*error).message).to_string_lossy().into_owned() };
                    gtk::glib::gobject_ffi::g_object_unref(job as *mut _);
                    Err(said)
                } else {
                    unsafe extern "C" fn sent(job: *mut GtkPrintJob, _: gpointer, error: *const GError) {
                        if !error.is_null() {
                            eprintln!("Print: {}", unsafe { std::ffi::CStr::from_ptr((*error).message) }.to_string_lossy());
                        }
                        unsafe { gtk::glib::gobject_ffi::g_object_unref(job as *mut _) };
                    }
                    gtk_print_job_send(job, Some(sent), std::ptr::null_mut(), None);
                    Ok(Printed::Sent)
                }
            }
        };
        gtk::ffi::gtk_widget_destroy(dialog);
        printed
    }
}
