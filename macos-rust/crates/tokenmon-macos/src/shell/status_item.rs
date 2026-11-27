//! The `NSStatusItem` in the menu bar: image, click target and anchor rect.
#![allow(unsafe_code)]

use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSImage, NSImageScaling, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::NSObjectProtocol;

use crate::placement::AppKitRect;

/// What the click target calls.
pub struct ClickIvars {
    on_click: Rc<dyn Fn()>,
}

define_class!(
    /// Receives the status button's action message.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "TMStatusClickTarget"]
    #[ivars = ClickIvars]
    pub struct ClickTarget;

    impl ClickTarget {
        #[unsafe(method(statusClicked:))]
        fn status_clicked(&self, _sender: Option<&AnyObject>) {
            (self.ivars().on_click)();
        }
    }

    unsafe impl NSObjectProtocol for ClickTarget {}
);

impl ClickTarget {
    fn new(mtm: MainThreadMarker, on_click: Rc<dyn Fn()>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ClickIvars { on_click });
        // SAFETY: `NSObject`'s designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// The menu bar item. Dropping it removes the item from the menu bar.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    _target: Retained<ClickTarget>,
}

impl StatusItem {
    /// Creates the item (variable length) whose button calls `on_click`.
    pub fn new(mtm: MainThreadMarker, on_click: Rc<dyn Fn()>) -> Option<Self> {
        let bar = NSStatusBar::systemStatusBar();
        let item = bar.statusItemWithLength(NSVariableStatusItemLength);
        let button = item.button(mtm)?;
        let target = ClickTarget::new(mtm, on_click);
        // SAFETY: `target` implements `statusClicked:` and is kept alive by
        // `StatusItem` for as long as the button can send the action.
        unsafe {
            button.setTarget(Some(&target));
            button.setAction(Some(sel!(statusClicked:)));
        }
        button.setImageScaling(NSImageScaling::ScaleNone);
        Some(Self {
            item,
            _target: target,
        })
    }

    /// Replaces the button image.
    pub fn set_image(&self, mtm: MainThreadMarker, image: &NSImage) {
        if let Some(button) = self.item.button(mtm) {
            button.setImage(Some(image));
        }
    }

    /// The button's window frame in AppKit screen coordinates: the anchor the
    /// panel opens under. `None` before the item has a window.
    pub fn anchor(&self, mtm: MainThreadMarker) -> Option<AppKitRect> {
        let frame = self.item.button(mtm)?.window()?.frame();
        Some(AppKitRect {
            x: frame.origin.x,
            y: frame.origin.y,
            width: frame.size.width,
            height: frame.size.height,
        })
    }

    /// Whether the menu bar is currently drawn with a dark appearance. The
    /// status bar window's appearance is the menu bar's (the Swift renderer
    /// walks to this same window); it follows the wallpaper tint, not only the
    /// system light/dark switch.
    pub fn is_dark(&self, mtm: MainThreadMarker) -> bool {
        use objc2_app_kit::NSAppearanceCustomization;
        self.item
            .button(mtm)
            .and_then(|b| b.window())
            .is_some_and(|w| appearance_is_dark(&w.effectiveAppearance()))
    }
}

/// Whether `appearance` is a dark one.
pub fn appearance_is_dark(appearance: &objc2_app_kit::NSAppearance) -> bool {
    use objc2_app_kit::{NSAppearanceNameAqua, NSAppearanceNameDarkAqua};
    use objc2_foundation::NSArray;
    // SAFETY: the two statics are valid appearance-name constants.
    let names = unsafe { NSArray::from_slice(&[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]) };
    appearance
        .bestMatchFromAppearancesWithNames(&names)
        .is_some_and(|n| n.to_string().contains("Dark"))
}
