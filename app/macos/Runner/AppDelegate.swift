import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  private var desktopChannel: FlutterMethodChannel?
  private var allowTermination = false

  override func applicationDidFinishLaunching(_ notification: Notification) {
    if let controller = mainFlutterWindow?.contentViewController as? FlutterViewController {
      let channel = FlutterMethodChannel(name: "openfader/desktop", binaryMessenger: controller.engine.binaryMessenger)
      desktopChannel = channel
      channel.setMethodCallHandler { [weak self] call, result in
        guard call.method == "terminate" else {
          result(FlutterMethodNotImplemented)
          return
        }
        self?.allowTermination = true
        result(nil)
        NSApp.terminate(nil)
      }
    }
    NSApp.setActivationPolicy(.regular)
    mainFlutterWindow?.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
  }

  override func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    if allowTermination { return .terminateNow }
    // The Flutter shell finalizes recording and shuts down its detached host
    // before allowing Cmd+Q or the application menu to terminate the process.
    desktopChannel?.invokeMethod("requestQuit", arguments: nil)
    return .terminateCancel
  }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return false
  }

  override func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
    if !flag { mainFlutterWindow?.makeKeyAndOrderFront(nil) }
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}
