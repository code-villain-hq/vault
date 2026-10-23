import XCTest
import CoreGraphics
@testable import VillainCaster

final class LockScreenTests: XCTestCase {
    func testLockPostsTheSystemShortcutInsteadOfSleepingTheDisplay() throws {
        var events: [CGEvent] = []
        try SystemActions.lockScreen(isTrusted: { true }, post: { events.append($0) })
        XCTAssertEqual(events.map { $0.type }, [.keyDown, .keyUp])
        for event in events {
            XCTAssertEqual(event.getIntegerValueField(.keyboardEventKeycode), 12)
            XCTAssertTrue(event.flags.contains([.maskControl, .maskCommand]))
        }
    }
    func testWithoutPermissionNoKeysArePosted() {
        var posted = false
        XCTAssertThrowsError(try SystemActions.lockScreen(isTrusted: { false }, post: { _ in posted = true }))
        XCTAssertFalse(posted)
    }
}
