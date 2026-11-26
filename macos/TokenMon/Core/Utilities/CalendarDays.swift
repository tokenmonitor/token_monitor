import Foundation

extension Calendar {
    /// `date` moved by `days` calendar days (negative moves back).
    ///
    /// Day arithmetic on a representable date does not fail in the calendars
    /// the app uses; should a calendar return nil, the result is `date`
    /// unchanged.
    func addingDays(_ days: Int, to date: Date) -> Date {
        self.date(byAdding: .day, value: days, to: date) ?? date
    }
}
