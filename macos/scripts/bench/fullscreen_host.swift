// A throwaway full-screen window (teal) used to test whether the Rust beta's
// panel can open over a full-screen app's Space. Run: swift fullscreen_host.swift
import AppKit

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let w = NSWindow(
    contentRect: NSRect(x: 100, y: 100, width: 800, height: 600),
    styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
w.title = "m0-fullscreen-test"
w.backgroundColor = .systemTeal
w.collectionBehavior = [.fullScreenPrimary]
w.makeKeyAndOrderFront(nil)
app.activate(ignoringOtherApps: true)
DispatchQueue.main.asyncAfter(deadline: .now() + 1) { w.toggleFullScreen(nil) }
DispatchQueue.main.asyncAfter(deadline: .now() + 16) { app.terminate(nil) }
app.run()
