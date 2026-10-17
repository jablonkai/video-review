//! Files opened from Finder ("Open With", double-click, drop on the Dock icon).
//! macOS delivers them as an `odoc` Apple Event, not in argv, and winit has no
//! handler for it, so the handler here queues the paths for the UI to pick up.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static REPAINT: OnceLock<egui::Context> = OnceLock::new();

/// Paths received since the last call.
pub fn take() -> Vec<PathBuf> {
    std::mem::take(&mut *OPENED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Lets later events wake the UI; until then they just wait in the queue.
pub fn set_repaint(ctx: &egui::Context) {
    let _ = REPAINT.set(ctx.clone());
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn push(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    OPENED.lock().unwrap_or_else(|e| e.into_inner()).extend(paths);
    if let Some(ctx) = REPAINT.get() {
        ctx.request_repaint();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install() {}

/// Call before the event loop starts. The handler goes in at
/// `applicationWillFinishLaunching`: AppKit installs its default `odoc` handler
/// before that, and a launch with documents sends the event right after it,
/// before `applicationDidFinishLaunching` (where eframe creates the app).
#[cfg(target_os = "macos")]
pub fn install() {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{AnyThread, msg_send, sel};
    use objc2_foundation::{NSAppleEventManager, NSNotification, NSNotificationCenter, NSString};
    use std::ptr::NonNull;

    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");

    let block = RcBlock::new(|_: NonNull<NSNotification>| {
        // The event manager doesn't retain its handler; it lives for the whole process.
        let handler: Retained<OpenHandler> = unsafe { msg_send![OpenHandler::alloc(), init] };
        let handler = Retained::into_raw(handler) as *const AnyObject;
        unsafe {
            NSAppleEventManager::sharedAppleEventManager().setEventHandler_andSelector_forEventClass_andEventID(
                &*handler,
                sel!(handleOpen:withReply:),
                CORE_EVENT_CLASS,
                OPEN_DOCUMENTS,
            );
        }
    });
    let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
    let observer = unsafe { NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block) };
    std::mem::forget(observer);
}

#[cfg(target_os = "macos")]
objc2::define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[name = "VideoReviewOpenHandler"]
    struct OpenHandler;

    impl OpenHandler {
        #[unsafe(method(handleOpen:withReply:))]
        fn handle_open(&self, event: &objc2_foundation::NSAppleEventDescriptor, _reply: &objc2_foundation::NSAppleEventDescriptor) {
            push(event_paths(event));
        }
    }
);

/// The file URLs in the event's direct object: a list, or a single item.
#[cfg(target_os = "macos")]
fn event_paths(event: &objc2_foundation::NSAppleEventDescriptor) -> Vec<PathBuf> {
    const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
    let Some(direct) = event.paramDescriptorForKeyword(DIRECT_OBJECT) else { return Vec::new() };
    let n = direct.numberOfItems();
    let items: Vec<_> = if n > 0 { (1..=n).filter_map(|i| direct.descriptorAtIndex(i)).collect() } else { vec![direct] };
    items.iter().filter_map(|d| d.fileURLValue()?.path()).map(|p| PathBuf::from(p.to_string())).collect()
}
