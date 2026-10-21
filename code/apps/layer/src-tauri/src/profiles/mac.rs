//! What a profile needs from AppKit: another copy brought forward (PROF-3),
//! and the Dock icon marked with the profile's initial (PROF-6). Every copy
//! is the same bundle, so the Dock shows the same icon, and the same name
//! on hover, for each; the initial is what tells them apart.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSBezierPath, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSImage, NSImageNameApplicationIcon, NSRunningApplication, NSStringDrawing,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

/// Bring the copy of the app running as `pid` to the front, all its
/// windows. Asked of the app in front, as this is when the user picks a
/// profile, so macOS allows it. NSRunningApplication is safe off the main
/// thread.
pub fn bring_forward(pid: i32) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows))
}

/// The colours an initial can sit on (PROF-10), in the order of
/// `--profile-<n>` in `theme.css`, which the sidebar draws with: one OKLCH
/// lightness, light enough for the initial in `INK`.
pub const COLOURS: [(f64, f64, f64); 8] = [
    (0.949, 0.533, 0.506),
    (0.439, 0.678, 0.984),
    (0.412, 0.761, 0.494),
    (0.855, 0.620, 0.247),
    (0.722, 0.592, 0.941),
    (0.067, 0.761, 0.792),
    (0.910, 0.529, 0.714),
    (0.620, 0.675, 0.745),
];

/// The initial, and the ring that cuts its disc out of the icon: Nightshade
/// night `bg`, the icon tile's own black (studio/brand/color.md), as `on-primary`
/// is in the sidebar. White read at under 3:1 on the discs.
const INK: (f64, f64, f64) = (11.0 / 255.0, 11.0 / 255.0, 11.0 / 255.0);

/// The colour of a profile that has not had one chosen: one of the first
/// six, by its id, so it keeps it when renamed. Six, not all of them: the
/// palette had six before colours could be chosen, and adding more must not
/// recolour every profile that never chose.
pub fn default_colour(id: &str) -> usize {
    id.bytes().fold(0, |h, b| (h * 31 + b as usize) % 6)
}

/// Put `name`'s initial on the Dock icon, on a disc in colour `colour`, or
/// put the plain icon back for `None`. Only on the main thread; elsewhere
/// it does nothing.
pub fn mark_dock(mark: Option<(&str, usize)>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // The bundle's own icon, never the one last set here.
    let Some(plain) = NSImage::imageNamed(unsafe { NSImageNameApplicationIcon }) else {
        return;
    };
    let Some((name, colour)) = mark else {
        unsafe { app.setApplicationIconImage(Some(&plain)) };
        return;
    };
    let initial: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    let image = marked(&plain, &initial, COLOURS[colour % COLOURS.len()]);
    unsafe { app.setApplicationIconImage(Some(&image)) };
}

/// `plain`, with `initial` on a disc in its lower right corner.
#[allow(deprecated)] // lockFocus: the drawing-handler alternative needs blocks, a dependency for one image.
fn marked(plain: &NSImage, initial: &str, (r, g, b): (f64, f64, f64)) -> Retained<NSImage> {
    let side = 512.0;
    let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(side, side));
    image.lockFocus();
    plain.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side)));

    let disc = side * 0.44;
    let at = NSPoint::new(side - disc - side * 0.02, side * 0.02);
    let ink = NSColor::colorWithSRGBRed_green_blue_alpha(INK.0, INK.1, INK.2, 1.0);
    ink.setFill();
    NSBezierPath::bezierPathWithOvalInRect(NSRect::new(at, NSSize::new(disc, disc))).fill();
    let inset = disc * 0.06;
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0).setFill();
    NSBezierPath::bezierPathWithOvalInRect(NSRect::new(
        NSPoint::new(at.x + inset, at.y + inset),
        NSSize::new(disc - 2.0 * inset, disc - 2.0 * inset),
    ))
    .fill();

    let font = NSFont::boldSystemFontOfSize(disc * 0.58);
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [&font, &ink];
    let attributes = NSDictionary::from_slices(&keys, &values);
    let text = NSString::from_str(initial);
    // SAFETY: the attributes are a font and a colour, under the keys for them.
    let size = unsafe { text.sizeWithAttributes(Some(&attributes)) };
    let point = NSPoint::new(at.x + (disc - size.width) / 2.0, at.y + (disc - size.height) / 2.0);
    unsafe { text.drawAtPoint_withAttributes(point, Some(&attributes)) };

    image.unlockFocus();
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_that_never_chose_keeps_one_of_the_first_six_colours() {
        assert_eq!(default_colour("personal"), default_colour("personal"));
        let all: std::collections::HashSet<_> =
            ["default", "personal", "acme", "work", "side", "x"].iter().map(|id| default_colour(id)).collect();
        assert!(all.len() > 1, "not one colour for everything");
        assert!(all.iter().all(|c| *c < 6));
        // What it was before there were eight, for the ids these two had.
        assert_eq!((default_colour("default"), default_colour("test-profile")), (3, 4));
    }
}
