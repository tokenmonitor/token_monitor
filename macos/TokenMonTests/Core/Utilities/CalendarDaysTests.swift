@testable import TokenMon
import XCTest

/// `Calendar.addingDays` moves by calendar days, so a day across a DST change
/// keeps its wall-clock time instead of drifting by an hour.
final class CalendarDaysTests: XCTestCase {
    func testAddingDaysKeepsMidnightAcrossADaylightSavingChange() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(identifier: "America/Denver"))
        let before = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 3, day: 7)))
        let after = calendar.addingDays(2, to: before)
        XCTAssertEqual(calendar.dateComponents([.month, .day, .hour], from: after), DateComponents(month: 3, day: 9, hour: 0))
        XCTAssertEqual(after.timeIntervalSince(before), 47 * 3600, "the spring-forward day is 23 hours long")
        XCTAssertEqual(calendar.addingDays(-2, to: after), before)
        XCTAssertEqual(calendar.addingDays(0, to: before), before)
    }
}
