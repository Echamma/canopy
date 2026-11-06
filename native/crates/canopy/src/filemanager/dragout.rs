//! Dragging files out of Canopy's window into any other app (a browser's upload page, Explorer, a chat), the way
//! Explorer hands them over. Windows runs the drag until the drop, so this returns once it's done.

use std::path::PathBuf;

/// Drags `files` (absolute paths) from the window in front, with `image` (PNG bytes) under the pointer if given.
/// Returns whether they were dropped somewhere.
#[cfg(windows)]
pub fn drag_files(files: Vec<PathBuf>, image: Option<Vec<u8>>) -> Result<bool, String> {
    use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
    use std::num::NonZeroIsize;

    struct Window(isize);
    impl HasWindowHandle for Window {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let hwnd = NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?;
            // SAFETY: the window is Canopy's own, alive for this call.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(hwnd))) })
        }
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetActiveWindow() -> isize;
    }
    // SAFETY: no arguments; it returns this thread's active window (egui runs on the window's thread).
    let window = Window(unsafe { GetActiveWindow() });
    let (sender, dropped) = std::sync::mpsc::channel();
    drag::start_drag(
        &window,
        drag::DragItem::Files(files),
        drag::Image::Raw(image.unwrap_or_default()),
        move |result, _| {
            let _ = sender.send(matches!(result, drag::DragResult::Dropped));
        },
        drag::Options::default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(dropped.try_recv().unwrap_or(false))
}

#[cfg(not(windows))]
pub fn drag_files(_files: Vec<PathBuf>, _image: Option<Vec<u8>>) -> Result<bool, String> {
    Err("Dragging files out works on Windows only for now.".into())
}

/// A small PNG of a picture for the drag (RGBA, `size` = [width, height]).
pub fn png(rgba: &[u8], size: [usize; 2]) -> Option<Vec<u8>> {
    let image = image::RgbaImage::from_raw(size[0] as u32, size[1] as u32, rgba.to_vec())?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png).ok()?;
    Some(bytes.into_inner())
}
