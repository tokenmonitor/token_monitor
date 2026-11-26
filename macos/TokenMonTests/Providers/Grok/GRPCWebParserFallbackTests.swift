@testable import TokenMon
import XCTest

/// GRPCWebParser paths the live payload does not take: a raw protobuf body
/// without gRPC-web framing, the used-% fallback when `[1, 1]` is absent, and
/// the debug field dump used to discover new payload paths.
final class GRPCWebParserFallbackTests: XCTestCase {
    private func fixed32(field: UInt8, _ value: Float) -> Data {
        var data = Data([field << 3 | 5])
        withUnsafeBytes(of: value.bitPattern.littleEndian) { data.append(contentsOf: $0) }
        return data
    }

    private func message(field: UInt8, _ body: Data) -> Data {
        Data([field << 3 | 2, UInt8(body.count)]) + body
    }

    private func varint(field: UInt8, _ value: UInt64) -> Data {
        var data = Data([field << 3])
        var rest = value
        repeat {
            var byte = UInt8(rest & 0x7F)
            rest >>= 7
            if rest != 0 { byte |= 0x80 }
            data.append(byte)
        } while rest != 0
        return data
    }

    /// Without `[1, 1]`, the shallowest field-1 float wins, and among equally
    /// deep ones the first in the payload.
    func testUsedPercentFallsBackToTheShallowestFieldOne() throws {
        let body = message(field: 2, fixed32(field: 1, 70))
            + message(field: 3, fixed32(field: 1, 80))
            + message(field: 4, fixed32(field: 1, 150)) // out of range, ignored
        XCTAssertEqual(try GRPCWebParser.parseUsage(body).usedPercent, 70, accuracy: 0.001)
        let withTopLevel = body + fixed32(field: 1, 12)
        XCTAssertEqual(try GRPCWebParser.parseUsage(withTopLevel).usedPercent, 12, accuracy: 0.001)
    }

    /// A billing period without any percent reads as 0% used.
    func testPeriodWithoutPercentReadsAsZero() throws {
        let reset: UInt64 = 2_000_000_000
        let body = message(field: 1, message(field: 5, varint(field: 1, reset)))
        let parsed = try GRPCWebParser.parseUsage(body, now: Date(timeIntervalSince1970: 1_900_000_000))
        XCTAssertEqual(parsed.usedPercent, 0)
        XCTAssertEqual(parsed.resetsAt, Date(timeIntervalSince1970: TimeInterval(reset)))
    }

    func testEmptyBodyIsAnError() {
        XCTAssertThrowsError(try GRPCWebParser.parseUsage(Data()))
    }

    func testDebugDumpListsEveryScannedField() {
        let reset: UInt64 = 2_000_000_000
        let body = message(field: 1, fixed32(field: 1, 42) + varint(field: 3, 7) + varint(field: 4, reset))
        let dump = GRPCWebParser.debugFieldDump(body)
        let lines = dump.components(separatedBy: "\n")
        XCTAssertEqual(lines.first, "frame[0] bytes=\(body.count)")
        XCTAssertTrue(lines.contains("  f32 1.1 = 42.0"))
        XCTAssertTrue(lines.contains("  vi  1.3 = 7"))
        XCTAssertTrue(lines.contains { $0.hasPrefix("  vi  1.4 = 2000000000 // ") }, "timestamps carry their date")
        XCTAssertEqual(GRPCWebParser.debugFieldDump(Data()), "")
    }
}
