//! Files dropped on the island.
//!
//! The webview can't take the drop itself: the window under the pointer is
//! WebView2's render window, which lives in the msedgewebview2 process, and
//! the drop target wry registers in our process never hears about the drag
//! (tested with a real OLE drag: no events, effect None). So Duri keeps its
//! own tiny window: native, in this process, invisible (alpha 1 of 255, the
//! lowest that still takes the pointer) and hidden by default. While a drag
//! is over the island, the hit-test thread puts it right over the island
//! (`place`), and its `IDropTarget` reports to the UI as `island://drag`:
//! `{ kind: "enter", paths }`, `{ kind: "leave" }`, `{ kind: "drop", paths }`.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DragEvent {
    Enter { paths: Vec<String> },
    Leave,
    Drop { paths: Vec<String> },
}

fn emit(app: &AppHandle, event: DragEvent) {
    let _ = app.emit_to(crate::island_window::LABEL, "island://drag", event);
}

/// Physical screen rectangle `(x, y, w, h)` of the catcher, or `None` to hide it.
pub type Placement = Option<(i32, i32, i32, i32)>;

#[cfg(windows)]
mod imp {
    use super::{emit, DragEvent, Placement};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use tauri::AppHandle;
    use windows::core::{implement, w, Ref, Result};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINTL, WPARAM};
    use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Ole::{
        IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, CF_HDROP, DROPEFFECT,
        DROPEFFECT_COPY, DROPEFFECT_NONE,
    };
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, SetLayeredWindowAttributes,
        SetWindowPos, TranslateMessage, HWND_TOPMOST, LWA_ALPHA, MSG, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_POPUP,
    };

    static HWND_RAW: AtomicIsize = AtomicIsize::new(0);

    /// File paths in a drag, or `None` when it carries no files (text, links).
    fn paths(data: Ref<'_, IDataObject>) -> Option<Vec<String>> {
        let data = data.as_ref()?;
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        unsafe {
            let mut medium = data.GetData(&format).ok()?;
            let hdrop = HDROP(medium.u.hGlobal.0 as _);
            let count = DragQueryFileW(hdrop, u32::MAX, None);
            let mut out = Vec::with_capacity(count as usize);
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                out.push(String::from_utf16_lossy(&buf[..len]));
            }
            ReleaseStgMedium(&mut medium);
            (!out.is_empty()).then_some(out)
        }
    }

    #[implement(IDropTarget)]
    struct Target {
        app: AppHandle,
        /// The drag in progress carries files.
        files: Cell<bool>,
    }

    impl IDropTarget_Impl for Target_Impl {
        fn DragEnter(
            &self,
            data: Ref<'_, IDataObject>,
            _keys: MODIFIERKEYS_FLAGS,
            _pt: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> Result<()> {
            let found = paths(data);
            self.files.set(found.is_some());
            unsafe { *effect = if found.is_some() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
            if let Some(paths) = found {
                emit(&self.app, DragEvent::Enter { paths });
            }
            Ok(())
        }

        fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
            unsafe { *effect = if self.files.get() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
            Ok(())
        }

        fn DragLeave(&self) -> Result<()> {
            if self.files.replace(false) {
                emit(&self.app, DragEvent::Leave);
            }
            Ok(())
        }

        fn Drop(
            &self,
            data: Ref<'_, IDataObject>,
            _keys: MODIFIERKEYS_FLAGS,
            _pt: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> Result<()> {
            self.files.set(false);
            match paths(data) {
                Some(paths) => {
                    unsafe { *effect = DROPEFFECT_COPY };
                    emit(&self.app, DragEvent::Drop { paths });
                }
                None => unsafe { *effect = DROPEFFECT_NONE },
            }
            place(None);
            Ok(())
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    pub fn spawn(app: AppHandle) {
        std::thread::Builder::new()
            .name("duri-drop".into())
            .spawn(move || unsafe {
                // OLE drag and drop needs a single-threaded apartment.
                if OleInitialize(None).is_err() {
                    log::error!("drop catcher: OleInitialize failed");
                    return;
                }
                let instance = GetModuleHandleW(None).unwrap_or_default();
                let class = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: instance.into(),
                    lpszClassName: w!("DuriDropCatcher"),
                    ..Default::default()
                };
                RegisterClassW(&class);
                let Ok(hwnd) = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                    w!("DuriDropCatcher"),
                    w!("Duri"),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                ) else {
                    log::error!("drop catcher window could not be created");
                    return;
                };
                // Alpha 0 would let the pointer (and the drag) through.
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA);
                let target: IDropTarget = Target { app, files: Cell::new(false) }.into();
                if RegisterDragDrop(hwnd, &target).is_err() {
                    log::error!("drop catcher: RegisterDragDrop failed");
                    return;
                }
                HWND_RAW.store(hwnd.0 as isize, Ordering::SeqCst);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .expect("failed to spawn drop catcher thread");
    }

    /// Puts the catcher over the island, or hides it. Cheap to call often:
    /// the caller only calls it when the placement changes.
    pub fn place(at: Placement) {
        let raw = HWND_RAW.load(Ordering::SeqCst);
        if raw == 0 {
            return;
        }
        let hwnd = HWND(raw as _);
        unsafe {
            let _ = match at {
                Some((x, y, w, h)) => {
                    SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW)
                }
                None => SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                ),
            };
        }
    }
}

#[cfg(windows)]
pub use imp::{place, spawn};

#[cfg(not(windows))]
pub fn spawn(_app: AppHandle) {}

#[cfg(not(windows))]
pub fn place(_at: Placement) {}
