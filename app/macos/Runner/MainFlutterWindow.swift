import AVFoundation
import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let availableSize = (self.screen ?? NSScreen.main)?.visibleFrame.size
    let windowFrame = NSRect(
      x: self.frame.minX, y: self.frame.minY,
      width: min(1440, availableSize?.width ?? 1440),
      height: min(1040, availableSize?.height ?? 1040))
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)
    self.minSize = NSSize(width: 720, height: 560)
    self.center()

    RegisterGeneratedPlugins(registry: flutterViewController)
    let audioPermission = FlutterMethodChannel(name: "openfader/audio_permission", binaryMessenger: flutterViewController.engine.binaryMessenger)
    // Keep a request in flight even if the Flutter caller stops waiting. TCC can
    // take arbitrarily long, and a second request must not create another prompt.
    var microphoneRequestPending = false
    func microphoneStatus() -> String {
      switch AVCaptureDevice.authorizationStatus(for: .audio) {
      case .authorized: return "authorized"
      case .denied: return "denied"
      case .restricted: return "restricted"
      case .notDetermined: return microphoneRequestPending ? "pending" : "notDetermined"
      @unknown default: return "restricted"
      }
    }
    audioPermission.setMethodCallHandler { call, result in
      DispatchQueue.main.async {
        switch call.method {
        case "microphoneStatus":
          result(microphoneStatus())
        case "requestMicrophone":
          guard microphoneStatus() == "notDetermined" else {
            result(microphoneStatus())
            return
          }
          microphoneRequestPending = true
          AVCaptureDevice.requestAccess(for: .audio) { _ in
            DispatchQueue.main.async {
              microphoneRequestPending = false
            }
          }
          result("pending")
        default:
          result(FlutterMethodNotImplemented)
        }
      }
    }

    super.awakeFromNib()
    self.makeKeyAndOrderFront(nil)
  }
}
