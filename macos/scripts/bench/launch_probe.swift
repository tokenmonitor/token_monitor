// Launches a program and prints the milliseconds from spawn until it logs
// "status item ready" on stderr (the Rust beta does so once its status item and
// first image exist), then the pid. CoreGraphics does not list status-item
// windows on recent macOS, so an in-process marker is the portable signal.
// Build: swiftc -O launch_probe.swift -o /tmp/launch_probe
// Usage: launch_probe <executable> [args...]   (env is inherited; the child keeps running)
import Foundation

let args = CommandLine.arguments.dropFirst()
guard let exe = args.first else { exit(2) }
let process = Process()
process.executableURL = URL(fileURLWithPath: exe)
process.arguments = Array(args.dropFirst())
let pipe = Pipe()
process.standardError = pipe
process.standardOutput = FileHandle(forWritingAtPath: "/dev/null")
let start = DispatchTime.now()
try process.run()
let pid = process.processIdentifier
let deadline = Date().addingTimeInterval(20)
var seen = ""
var done = false
let reader = pipe.fileHandleForReading
reader.readabilityHandler = { handle in
    let data = handle.availableData
    if data.isEmpty { return }
    seen += String(decoding: data, as: UTF8.self)
    if !done, seen.contains("status item ready") {
        done = true
        let ms = Double(DispatchTime.now().uptimeNanoseconds - start.uptimeNanoseconds) / 1e6
        print("pid=\(pid) status_item_ms=\(String(format: "%.0f", ms))")
        fflush(stdout)
        // Keep draining so the child never blocks on a full pipe, then detach.
        exit(0)
    }
}
while !done && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.01)) }
if !done { print("pid=\(pid) status_item_ms=timeout"); exit(1) }
