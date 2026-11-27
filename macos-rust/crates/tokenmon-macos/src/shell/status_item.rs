//! The `NSStatusItem` in the menu bar: image, click target, context menu and
//! anchor rect.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSEventMask, NSEventModifierFlags, NSEventType, NSImage, NSImageScaling, NSMenu,
    NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSString};

use crate::placement::AppKitRect;

/// Called with the index of the chosen menu item.
pub type PickFn = Rc<dyn Fn(usize)>;

/// One row of the context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuRow {
    /// A clickable item (its index in the slice is what `on_pick` receives).
    Item(String),
    /// A separator.
    Separator,
}

/// What the targets call.
pub struct TargetIvars {
    on_click: Rc<dyn Fn(bool)>,
    on_pick: RefCell<Option<PickFn>>,
}

define_class!(
    /// Receives the status button's action and the menu items' actions.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "TMStatusTarget"]
    #[ivars = TargetIvars]
    pub struct StatusTarget;

    impl StatusTarget {
        #[unsafe(method(statusClicked:))]
        fn status_clicked(&self, _sender: Option<&AnyObject>) {
            let app = NSApplication::sharedApplication(self.mtm());
            let right = app.currentEvent().is_some_and(|e| {
                e.r#type() == NSEventType::RightMouseUp
                    || e.modifierFlags().contains(NSEventModifierFlags::Control)
            });
            (self.ivars().on_click)(right);
        }

        #[unsafe(method(menuPicked:))]
        fn menu_picked(&self, sender: Option<&NSMenuItem>) {
            let Some(item) = sender else { return };
            let pick = self.ivars().on_pick.borrow().clone();
            if let (Some(pick), Ok(index)) = (pick, usize::try_from(item.tag())) {
                pick(index);
            }
        }
    }

    unsafe impl NSObjectProtocol for StatusTarget {}
);

impl StatusTarget {
    fn new(mtm: MainThreadMarker, on_click: Rc<dyn Fn(bool)>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars {
            on_click,
            on_pick: RefCell::new(None),
        });
        // SAFETY: `NSObject`'s designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// The menu bar item. Dropping it removes the item from the menu bar.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    target: Retained<StatusTarget>,
}

impl StatusItem {
    /// Creates the item (variable length). `on_click(true)` is a right or
    /// control click.
    pub fn new(mtm: MainThreadMarker, on_click: Rc<dyn Fn(bool)>) -> Option<Self> {
        let bar = NSStatusBar::systemStatusBar();
        let item = bar.statusItemWithLength(NSVariableStatusItemLength);
        let button = item.button(mtm)?;
        let target = StatusTarget::new(mtm, on_click);
        // SAFETY: `target` implements `statusClicked:` and is kept alive by
        // `StatusItem` for as long as the button can send the action.
        unsafe {
            button.setTarget(Some(&target));
            button.setAction(Some(sel!(statusClicked:)));
        }
        let _ = button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);
        button.setImageScaling(NSImageScaling::ScaleNone);
        Some(Self { item, target })
    }

    /// Replaces the button image.
    pub fn set_image(&self, mtm: MainThreadMarker, image: &NSImage) {
        if let Some(button) = self.item.button(mtm) {
            button.setImage(Some(image));
        }
    }

    /// Pops up the context menu under the button; `on_pick` gets the index
    /// (within `rows`) of the chosen item.
    pub fn show_menu(&self, mtm: MainThreadMarker, rows: &[MenuRow], on_pick: Rc<dyn Fn(usize)>) {
        let Some(button) = self.item.button(mtm) else {
            return;
        };
        *self.target.ivars().on_pick.borrow_mut() = Some(on_pick);
        let menu = NSMenu::new(mtm);
        for (index, row) in rows.iter().enumerate() {
            match row {
                MenuRow::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
                MenuRow::Item(label) => {
                    // SAFETY: `menuPicked:` is implemented by the target kept alive in `self`.
                    let item = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            NSMenuItem::alloc(mtm),
                            &NSString::from_str(label),
                            Some(sel!(menuPicked:)),
                            &NSString::from_str(""),
                        )
                    };
                    item.setTag(isize::try_from(index).unwrap_or(0));
                    // SAFETY: as above.
                    unsafe { item.setTarget(Some(&self.target)) };
                    menu.addItem(&item);
                }
            }
        }
        let height = button.bounds().size.height;
        let _ = menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, height + 4.0),
            Some(&button),
        );
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
